//! The advert lifecycle on an `Engine` (issue #280, with the sign-off in #279).
//!
//! On a client, the advert registration this node last made is remembered and repeated every
//! third of the time to live, so relays keep it; it is forgotten (and the refresh stopped) before
//! a sign-off. On a relay, a sweep deletes client adverts older than the time to live.

use super::Engine;
use crate::ddb::DdbBackend;
use crate::themes;

impl Engine {
    /// Record the advert registration this node just made, so the advert refresh can repeat it
    /// (issue #280). The first client registration starts the refresh loop, and a later one (for
    /// example after a relay switch) replaces what it repeats. A relay registration stops it:
    /// relays never expire relay adverts, so there is nothing to refresh.
    pub(crate) fn note_advert_registered(&self, registration: crate::ddb::AdvertRegistration) {
        let is_relay = registration.is_relay();
        match self.advert_registration.lock() {
            Ok(mut g) => *g = Some(registration),
            Err(_) => {
                tracing::error!(
                    "[Engine::note_advert_registered] advert_registration mutex poisoned"
                );
                return;
            }
        }
        if is_relay {
            self.stop_advert_refresh();
        } else {
            self.start_advert_refresh();
        }
    }

    /// Start the client advert refresh loop if adverts expire and it is not already running.
    fn start_advert_refresh(&self) {
        let Some(ttl) = self.advert_ttl else {
            return;
        };
        let Ok(mut guard) = self.advert_refresher.lock() else {
            tracing::error!("[Engine::start_advert_refresh] advert_refresher mutex poisoned");
            return;
        };
        if guard.is_some() {
            return;
        }
        let api_weak = self.bingle_api.clone();
        let span = self.span();
        let pass: crate::util::periodic_task::PeriodicPass = Box::new(move || {
            let _guard = span.enter();
            let Some(api) = api_weak.upgrade() else {
                return false;
            };
            let mut work = None;
            api.with_engine(&mut |e| {
                let registration = e.advert_registration.lock().ok().and_then(|g| g.clone());
                work = registration.map(|r| (r, e.ddb_client()));
            });
            // Register outside with_engine: it waits on a relay's response.
            if let Some((registration, client)) = work {
                match registration.register(client.as_ref()) {
                    Ok(()) => tracing::debug!(
                        "[Engine::advert_refresh] refreshed advert {:?}",
                        registration
                    ),
                    // The next pass retries; the advert lapses only if every pass in a time to
                    // live fails, as for a node that has gone.
                    Err(e) => tracing::info!(
                        "[Engine::advert_refresh] advert refresh failed, retrying next interval: {}",
                        e
                    ),
                }
            }
            true
        });
        let mut task = crate::util::periodic_task::PeriodicTask::new(
            "AdvertRefresh",
            ttl / 3,
            crate::util::periodic_task::FirstPass::AfterInterval,
            pass,
        );
        task.start();
        *guard = Some(task);
    }

    /// Forget this node's advert registration and stop refreshing it, returning what was
    /// registered. Used before a DDB sign-off (#279), so a refresh pass can't re-add the advert;
    /// an in-flight pass finishes first.
    pub(crate) fn take_advert_registration(&self) -> Option<crate::ddb::AdvertRegistration> {
        self.stop_advert_refresh();
        self.advert_registration
            .lock()
            .ok()
            .and_then(|mut g| g.take())
    }

    /// Stop the client advert refresh loop, if running.
    pub(crate) fn stop_advert_refresh(&self) {
        let old = self.advert_refresher.lock().ok().and_then(|mut g| g.take());
        if let Some(mut old) = old {
            old.stop();
        }
    }

    /// On a relay, start the loop that deletes client adverts older than the time to live
    /// (issue #280), and mark this relay's own advert as never expiring.
    pub(crate) fn start_advert_expiry_sweep(&self) {
        if let Ok(iss) = self.issuer()
            && let Ok(mut b) = self.ddb_backend.lock()
        {
            b.set_own_id(iss.trim_end_matches(crate::protocol::ISSUER_SUFFIX));
        }
        let Some(ttl) = self.advert_ttl else {
            return;
        };
        let Ok(mut guard) = self.advert_sweeper.lock() else {
            tracing::error!("[Engine::start_advert_expiry_sweep] advert_sweeper mutex poisoned");
            return;
        };
        if let Some(mut old) = guard.take() {
            old.stop();
        }
        let api_weak = self.bingle_api.clone();
        let span = self.span();
        let pass: crate::util::periodic_task::PeriodicPass = Box::new(move || {
            let _guard = span.enter();
            let Some(api) = api_weak.upgrade() else {
                return false;
            };
            api.with_engine(&mut |e| {
                e.sweep_expired_adverts();
            });
            true
        });
        // Lookups already hide expired adverts, so the sweep only reclaims memory; half the time
        // to live bounds how long an expired advert is held.
        let mut task = crate::util::periodic_task::PeriodicTask::new(
            "AdvertExpirySweep",
            ttl / 2,
            crate::util::periodic_task::FirstPass::AfterInterval,
            pass,
        );
        task.start();
        *guard = Some(task);
    }

    /// Stop the relay advert expiry sweep, if running.
    pub(crate) fn stop_advert_expiry_sweep(&self) {
        let old = self.advert_sweeper.lock().ok().and_then(|mut g| g.take());
        if let Some(mut old) = old {
            old.stop();
        }
    }

    /// Delete expired client adverts from this node's DDB (issue #280). Returns the deleted ids.
    pub fn sweep_expired_adverts(&self) -> Vec<String> {
        let expired = match self.ddb_backend.lock() {
            Ok(mut b) => b.sweep_expired(),
            Err(_) => {
                tracing::error!("[Engine::sweep_expired_adverts] DDB backend lock poisoned");
                return Vec::new();
            }
        };
        if !expired.is_empty() {
            info_theme!(
                themes::ENGINE,
                "[Engine::sweep_expired_adverts] removed {} expired advert(s): {:?}",
                expired.len(),
                expired
            );
        }
        expired
    }

    /// Test helper: the advert time to live in effect (#280).
    pub fn advert_ttl_for_tests(&self) -> Option<std::time::Duration> {
        self.advert_ttl
    }

    /// Test helper: whether the client advert refresh loop is running (#280).
    pub fn advert_refresh_running_for_tests(&self) -> bool {
        self.advert_refresher
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    /// Test helper: stop the client advert refresh loop (#280).
    pub fn stop_advert_refresh_for_tests(&self) {
        self.stop_advert_refresh();
    }

    /// Test helper: whether the relay advert expiry sweep is running (#280).
    pub fn advert_sweep_running_for_tests(&self) -> bool {
        self.advert_sweeper
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    /// Test helper: record an advert registration as if it had just been made (#280).
    pub fn note_advert_registered_for_tests(&self, registration: crate::ddb::AdvertRegistration) {
        self.note_advert_registered(registration);
    }

    /// Test helper: the advert registration the refresh repeats, if any (#280).
    pub fn advert_registration_for_tests(&self) -> Option<crate::ddb::AdvertRegistration> {
        self.advert_registration.lock().ok().and_then(|g| g.clone())
    }
}
