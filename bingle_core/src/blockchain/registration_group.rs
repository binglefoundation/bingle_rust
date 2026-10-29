//! Checking and co-signing the atomic transaction groups the registration service builds for an
//! app-store registration (issue #344).
//!
//! The service (bingle_registrar) returns a whole unsigned group, each transaction as base64
//! msgpack exactly as algosdk's `encodeUnsignedTransaction` writes it, and the positions the
//! client signs (`sign_indexes`). Before signing, every client transaction is checked against the
//! expected template: a transaction with an unexpected type, receiver, amount, asset, app, argument,
//! fee, rekey, close-to, note or lease, or any field the template doesn't use, is refused.
//!
//! The client never re-encodes a transaction: it signs `"TX"` followed by the bytes it was given,
//! and returns them wrapped as a signed transaction, so the transaction ID the service checks can't
//! drift through a lossy decode.
//!
//! **Registration** (client signs 1, 3, 4 and 5; an invite adds service transaction 7):
//! 0 funder pays the address; 1 Bingle$ opt-in; 2 funder sends 1 Bingle$; 3 Bingle DApp opt-in;
//! 4 1 Bingle$ to the Bingle DApp; 5 `register(handle)`; 6 the service's registrar `claim`.
//!
//! **Handle clash recovery** (old key signs 0–2, new key 3 and 5–7; an invite adds 9):
//! 0 old key's Bingle DApp ClearState; 1 old key's Bingle$ close-out to the funder; 2 old key's Algo
//! close to the new address; 3 new key's Bingle$ opt-in; 4 funder sends 1 Bingle$ to the new
//! address and pays every client fee; 5 Bingle DApp opt-in; 6 1 Bingle$ to the Bingle DApp;
//! 7 `register(handle)`; 8 the service's registrar `rebind`.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::Signer;
/// The key type the signing functions take; see [`signing_key`].
pub use ed25519_dalek::SigningKey;
use serde::Deserialize;
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha512_256};

/// Positions the client signs in a registration group.
pub const REGISTRATION_SIGN_INDEXES: [usize; 4] = [1, 3, 4, 5];

/// Positions the client signs in a handle clash recovery group: the old key signs
/// [`RECOVERY_OLD_KEY_INDEXES`], the new key the rest.
pub const RECOVERY_SIGN_INDEXES: [usize; 7] = [0, 1, 2, 3, 5, 6, 7];

/// Positions in a recovery group the old key signs.
pub const RECOVERY_OLD_KEY_INDEXES: [usize; 3] = [0, 1, 2];

/// The recovery group's funder transaction: 1 Bingle$ to the new address, paying the client fees.
const RECOVERY_FUNDER_INDEX: usize = 4;

/// The most a client transaction may pay in fees, in microAlgo: the network's minimum fee.
/// Registration transactions each pay it from the funding; recovery transactions pay 0 because
/// the funder pools their fees.
pub const MAX_CLIENT_FEE: u64 = 1000;

/// The Bingle DApp method a registration calls.
const REGISTER_METHOD: &str = "register(string)void";

/// Application call `OnComplete` values (`apan`).
const ON_COMPLETE_OPT_IN: u64 = 1;
const ON_COMPLETE_CLEAR_STATE: u64 = 3;

/// The Bingle DApp and Bingle$ asset the group must use, from the client's own network config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistrationGroupIds {
    /// The Bingle DApp's application ID.
    pub bingle_app_id: u64,
    /// The Bingle$ asset ID.
    pub bingle_asset_id: u64,
}

/// Why a group was refused. Nothing is signed when any transaction fails its check.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistrationGroupError {
    /// The group has the wrong number of transactions.
    #[error("the group has {actual} transactions; expected {expected}")]
    GroupSize {
        /// The sizes allowed, such as "7 or 8".
        expected: String,
        /// The number of transactions given.
        actual: usize,
    },
    /// The positions to sign are not the template's.
    #[error("sign_indexes {actual:?} don't match the expected {expected:?}")]
    SignIndexes {
        /// The template's positions.
        expected: Vec<usize>,
        /// The positions given.
        actual: Vec<u32>,
    },
    /// A transaction isn't valid base64 msgpack, or has a field the template doesn't use.
    #[error("transaction {index} can't be decoded: {reason}")]
    Decode {
        /// The transaction's position in the group.
        index: usize,
        /// The decoder's message.
        reason: String,
    },
    /// A transaction doesn't match its template.
    #[error("transaction {index} is not the expected {expected}: {reason}")]
    Template {
        /// The transaction's position in the group.
        index: usize,
        /// What the template expects there.
        expected: &'static str,
        /// What differs.
        reason: String,
    },
    /// A transaction has no group ID, or a different one from the first transaction's.
    #[error("transaction {index} is not in the same group as the rest")]
    Group {
        /// The transaction's position in the group.
        index: usize,
    },
}

/// One transaction's msgpack fields, as Algorand names them. Fields the client templates never use
/// (programs, schemas, boxes, key registration, asset configuration and freeze, and so on) are not
/// listed, so a transaction carrying any of them fails to decode and is refused.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTransaction {
    #[serde(rename = "type")]
    txn_type: String,
    snd: ByteBuf,
    fee: Option<u64>,
    // The validity window, network and group: allowed, and not checked here (the group ID is checked
    // across the whole group by `decode_group`).
    #[allow(dead_code)]
    fv: Option<u64>,
    #[allow(dead_code)]
    lv: u64,
    #[allow(dead_code)]
    #[serde(rename = "gen")]
    genesis_id: Option<String>,
    #[allow(dead_code)]
    gh: ByteBuf,
    #[allow(dead_code)]
    grp: Option<ByteBuf>,
    lx: Option<ByteBuf>,
    note: Option<ByteBuf>,
    rekey: Option<ByteBuf>,
    // payment
    rcv: Option<ByteBuf>,
    amt: Option<u64>,
    close: Option<ByteBuf>,
    // asset transfer
    xaid: Option<u64>,
    aamt: Option<u64>,
    arcv: Option<ByteBuf>,
    aclose: Option<ByteBuf>,
    asnd: Option<ByteBuf>,
    // application call
    apid: Option<u64>,
    apan: Option<u64>,
    apaa: Option<Vec<ByteBuf>>,
    apat: Option<Vec<ByteBuf>>,
    apfa: Option<Vec<u64>>,
    apas: Option<Vec<u64>>,
}

/// Only the group ID, for transactions the client doesn't sign or check (the service's).
#[derive(Debug, Deserialize)]
struct GroupOnly {
    grp: Option<ByteBuf>,
}

/// A decoded transaction with the bytes it came from.
struct GroupTransaction {
    bytes: Vec<u8>,
}

/// Checks a registration group and signs the client's four transactions with `key`, the new
/// account's key. Returns the signed transactions as base64 msgpack, in `sign_indexes` order,
/// ready for the service's `signed_transactions`.
pub fn sign_registration_group(
    transactions: &[String],
    sign_indexes: &[u32],
    ids: RegistrationGroupIds,
    handle: &str,
    key: &SigningKey,
) -> Result<Vec<String>, RegistrationGroupError> {
    check_size(transactions, &[7, 8])?;
    check_sign_indexes(sign_indexes, &REGISTRATION_SIGN_INDEXES)?;
    let group = decode_group(transactions)?;
    let address = public_key(key);

    let expected = |index| check_client_basics(&group, index, &address);
    let opt_in = expected(1)?;
    check_asset_opt_in(1, &opt_in, ids)?;
    let app_opt_in = expected(3)?;
    check_app_call(3, &app_opt_in, ids, Some(ON_COMPLETE_OPT_IN), None)?;
    let payment = expected(4)?;
    check_asset_transfer(4, &payment, ids, &app_address(ids.bingle_app_id), 1, None)?;
    let register = expected(5)?;
    check_app_call(5, &register, ids, None, Some(handle))?;

    Ok(REGISTRATION_SIGN_INDEXES
        .iter()
        .map(|&index| sign(&group[index].bytes, key))
        .collect())
}

/// Checks a handle clash recovery group and signs the client's transactions: 0–2 with `old_key`
/// (the account that lost its handle) and 3, 5, 6 and 7 with `new_key` (the fresh account it moves
/// to). The funder is taken from the group: the sender of transaction 4, its 1 Bingle$ transfer to
/// the new address. Only it may receive the old account's Bingle$ close-out.
pub fn sign_recovery_group(
    transactions: &[String],
    sign_indexes: &[u32],
    ids: RegistrationGroupIds,
    handle: &str,
    old_key: &SigningKey,
    new_key: &SigningKey,
) -> Result<Vec<String>, RegistrationGroupError> {
    check_size(transactions, &[9, 10])?;
    check_sign_indexes(sign_indexes, &RECOVERY_SIGN_INDEXES)?;
    let group = decode_group(transactions)?;
    let old = public_key(old_key);
    let new = public_key(new_key);
    if old == new {
        return Err(template(
            RECOVERY_FUNDER_INDEX,
            "recovery group",
            "the old and new keys are the same",
        ));
    }

    let funder_txn = decode(RECOVERY_FUNDER_INDEX, &group[RECOVERY_FUNDER_INDEX].bytes)?;
    let funder = funder_txn.snd.to_vec();
    if funder == old || funder == new {
        return Err(template(
            RECOVERY_FUNDER_INDEX,
            "funder transfer",
            "the funder is one of the client's own addresses",
        ));
    }
    check_asset_transfer(RECOVERY_FUNDER_INDEX, &funder_txn, ids, &new, 1, None)?;

    let from_old = |index| check_client_basics(&group, index, &old);
    let from_new = |index| check_client_basics(&group, index, &new);

    let clear_state = from_old(0)?;
    check_app_call(0, &clear_state, ids, Some(ON_COMPLETE_CLEAR_STATE), None)?;
    let close_out = from_old(1)?;
    check_asset_transfer(1, &close_out, ids, &funder, 0, Some(&funder))?;
    let close = from_old(2)?;
    check_payment_close(2, &close, &new)?;
    let opt_in = from_new(3)?;
    check_asset_opt_in(3, &opt_in, ids)?;
    let app_opt_in = from_new(5)?;
    check_app_call(5, &app_opt_in, ids, Some(ON_COMPLETE_OPT_IN), None)?;
    let payment = from_new(6)?;
    check_asset_transfer(6, &payment, ids, &app_address(ids.bingle_app_id), 1, None)?;
    let register = from_new(7)?;
    check_app_call(7, &register, ids, None, Some(handle))?;

    Ok(RECOVERY_SIGN_INDEXES
        .iter()
        .map(|&index| {
            let key = if RECOVERY_OLD_KEY_INDEXES.contains(&index) {
                old_key
            } else {
                new_key
            };
            sign(&group[index].bytes, key)
        })
        .collect())
}

/// The signing key for an account's 25-word recovery phrase (or a `b64:` secret), for the signing
/// functions above.
pub fn signing_key(passphrase: &str) -> anyhow::Result<SigningKey> {
    Ok(SigningKey::from_bytes(
        &algo_ops::AlgoOps::seed_from_passphrase(passphrase)?,
    ))
}

/// The Algorand address bytes of an application's account: SHA-512/256 of `"appID"` and the app
/// ID as a big-endian `u64`.
pub fn app_address(app_id: u64) -> Vec<u8> {
    let mut hasher = Sha512_256::new();
    hasher.update(b"appID");
    hasher.update(app_id.to_be_bytes());
    hasher.finalize().to_vec()
}

/// The ABI-encoded arguments of `register(handle)`: the 4-byte method selector, then the handle as
/// an ABI string (a big-endian `u16` byte length and the UTF-8 bytes, as typed).
pub fn register_args(handle: &str) -> Vec<Vec<u8>> {
    let selector = Sha512_256::digest(REGISTER_METHOD.as_bytes())[..4].to_vec();
    let bytes = handle.as_bytes();
    let mut encoded = Vec::with_capacity(2 + bytes.len());
    // A handle longer than u16::MAX bytes can't be encoded; the length check below then fails.
    encoded.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_be_bytes());
    encoded.extend_from_slice(bytes);
    vec![selector, encoded]
}

fn check_size(transactions: &[String], allowed: &[usize]) -> Result<(), RegistrationGroupError> {
    if allowed.contains(&transactions.len()) {
        return Ok(());
    }
    Err(RegistrationGroupError::GroupSize {
        expected: allowed
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(" or "),
        actual: transactions.len(),
    })
}

fn check_sign_indexes(
    sign_indexes: &[u32],
    expected: &[usize],
) -> Result<(), RegistrationGroupError> {
    let matches = sign_indexes.len() == expected.len()
        && sign_indexes
            .iter()
            .zip(expected)
            .all(|(&actual, &wanted)| usize::try_from(actual).ok() == Some(wanted));
    if matches {
        return Ok(());
    }
    Err(RegistrationGroupError::SignIndexes {
        expected: expected.to_vec(),
        actual: sign_indexes.to_vec(),
    })
}

/// Decodes every transaction's base64, and checks they all carry the same group ID.
fn decode_group(transactions: &[String]) -> Result<Vec<GroupTransaction>, RegistrationGroupError> {
    let mut group = Vec::with_capacity(transactions.len());
    let mut group_id: Option<Vec<u8>> = None;
    for (index, encoded) in transactions.iter().enumerate() {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|e| RegistrationGroupError::Decode {
                index,
                reason: e.to_string(),
            })?;
        let only: GroupOnly =
            rmp_serde::from_slice(&bytes).map_err(|e| RegistrationGroupError::Decode {
                index,
                reason: e.to_string(),
            })?;
        let Some(grp) = only.grp else {
            return Err(RegistrationGroupError::Group { index });
        };
        match &group_id {
            None => group_id = Some(grp.to_vec()),
            Some(first) if *first == grp.to_vec() => {}
            Some(_) => return Err(RegistrationGroupError::Group { index }),
        }
        group.push(GroupTransaction { bytes });
    }
    Ok(group)
}

fn decode(index: usize, bytes: &[u8]) -> Result<RawTransaction, RegistrationGroupError> {
    rmp_serde::from_slice(bytes).map_err(|e| RegistrationGroupError::Decode {
        index,
        reason: e.to_string(),
    })
}

/// Decodes a client transaction strictly and checks what every client transaction shares: the
/// sender, a fee no more than [`MAX_CLIENT_FEE`], and no rekey, note or lease.
fn check_client_basics(
    group: &[GroupTransaction],
    index: usize,
    sender: &[u8],
) -> Result<RawTransaction, RegistrationGroupError> {
    let txn = decode(index, &group[index].bytes)?;
    let what = "client transaction";
    if txn.snd.as_slice() != sender {
        return Err(template(index, what, "the sender is not this account"));
    }
    if txn.fee.unwrap_or(0) > MAX_CLIENT_FEE {
        return Err(template(
            index,
            what,
            &format!(
                "the fee {} is more than {MAX_CLIENT_FEE}",
                txn.fee.unwrap_or(0)
            ),
        ));
    }
    if txn.rekey.is_some() {
        return Err(template(index, what, "it rekeys the account"));
    }
    if txn.note.is_some() {
        return Err(template(index, what, "it has a note"));
    }
    if txn.lx.is_some() {
        return Err(template(index, what, "it has a lease"));
    }
    Ok(txn)
}

/// A 0 Bingle$ transfer to itself: the Bingle$ opt-in.
fn check_asset_opt_in(
    index: usize,
    txn: &RawTransaction,
    ids: RegistrationGroupIds,
) -> Result<(), RegistrationGroupError> {
    let sender = txn.snd.to_vec();
    check_asset_transfer(index, txn, ids, &sender, 0, None)
}

/// A Bingle$ transfer of exactly `amount` to `receiver`, closing to `close_to` or not at all.
fn check_asset_transfer(
    index: usize,
    txn: &RawTransaction,
    ids: RegistrationGroupIds,
    receiver: &[u8],
    amount: u64,
    close_to: Option<&[u8]>,
) -> Result<(), RegistrationGroupError> {
    let what = "Bingle$ transfer";
    if txn.txn_type != "axfer" {
        return Err(template(
            index,
            what,
            &format!("its type is {}", txn.txn_type),
        ));
    }
    only_fields(
        index,
        what,
        &[
            ("rcv", txn.rcv.is_some()),
            ("amt", txn.amt.is_some()),
            ("close", txn.close.is_some()),
            ("asnd", txn.asnd.is_some()),
        ],
    )?;
    no_app_fields(index, what, txn)?;
    if txn.xaid != Some(ids.bingle_asset_id) {
        return Err(template(
            index,
            what,
            &format!("the asset is {:?}", txn.xaid),
        ));
    }
    if txn.arcv.as_deref().map(|r| r.as_slice()) != Some(receiver) {
        return Err(template(index, what, "the receiver is unexpected"));
    }
    if txn.aamt.unwrap_or(0) != amount {
        return Err(template(
            index,
            what,
            &format!("the amount is {}, not {amount}", txn.aamt.unwrap_or(0)),
        ));
    }
    if txn.aclose.as_deref().map(|c| c.as_slice()) != close_to {
        return Err(template(index, what, "the close-to address is unexpected"));
    }
    Ok(())
}

/// A 0 Algo payment to `receiver` that closes the account to it.
fn check_payment_close(
    index: usize,
    txn: &RawTransaction,
    receiver: &[u8],
) -> Result<(), RegistrationGroupError> {
    let what = "account close";
    if txn.txn_type != "pay" {
        return Err(template(
            index,
            what,
            &format!("its type is {}", txn.txn_type),
        ));
    }
    only_fields(
        index,
        what,
        &[
            ("xaid", txn.xaid.is_some()),
            ("aamt", txn.aamt.is_some()),
            ("arcv", txn.arcv.is_some()),
            ("aclose", txn.aclose.is_some()),
            ("asnd", txn.asnd.is_some()),
        ],
    )?;
    no_app_fields(index, what, txn)?;
    if txn.rcv.as_deref().map(|r| r.as_slice()) != Some(receiver) {
        return Err(template(index, what, "the receiver is not the new address"));
    }
    if txn.amt.unwrap_or(0) != 0 {
        return Err(template(index, what, "the amount is not 0"));
    }
    if txn.close.as_deref().map(|c| c.as_slice()) != Some(receiver) {
        return Err(template(index, what, "it doesn't close to the new address"));
    }
    Ok(())
}

/// A Bingle DApp call: with `on_complete` (`None` for NoOp) and, for `register`, the handle.
/// Only `register` may carry arguments and a foreign asset (Bingle$); nothing else is allowed.
fn check_app_call(
    index: usize,
    txn: &RawTransaction,
    ids: RegistrationGroupIds,
    on_complete: Option<u64>,
    register_handle: Option<&str>,
) -> Result<(), RegistrationGroupError> {
    let what = match (on_complete, register_handle) {
        (Some(ON_COMPLETE_OPT_IN), _) => "Bingle DApp opt-in",
        (Some(ON_COMPLETE_CLEAR_STATE), _) => "Bingle DApp ClearState",
        _ => "Bingle DApp register(handle)",
    };
    if txn.txn_type != "appl" {
        return Err(template(
            index,
            what,
            &format!("its type is {}", txn.txn_type),
        ));
    }
    only_fields(
        index,
        what,
        &[
            ("rcv", txn.rcv.is_some()),
            ("amt", txn.amt.is_some()),
            ("close", txn.close.is_some()),
            ("xaid", txn.xaid.is_some()),
            ("aamt", txn.aamt.is_some()),
            ("arcv", txn.arcv.is_some()),
            ("aclose", txn.aclose.is_some()),
            ("asnd", txn.asnd.is_some()),
            ("apat", txn.apat.is_some()),
            ("apfa", txn.apfa.is_some()),
        ],
    )?;
    if txn.apid != Some(ids.bingle_app_id) {
        return Err(template(index, what, &format!("the app is {:?}", txn.apid)));
    }
    if txn.apan != on_complete {
        return Err(template(
            index,
            what,
            &format!("its on-complete is {:?}", txn.apan),
        ));
    }
    match register_handle {
        None => only_fields(
            index,
            what,
            &[("apaa", txn.apaa.is_some()), ("apas", txn.apas.is_some())],
        ),
        Some(handle) => {
            let args: Option<Vec<Vec<u8>>> = txn
                .apaa
                .as_ref()
                .map(|args| args.iter().map(|a| a.to_vec()).collect());
            if args.as_ref() != Some(&register_args(handle)) {
                return Err(template(
                    index,
                    what,
                    "the arguments are not register with this handle",
                ));
            }
            if txn.apas.as_deref() != Some(&[ids.bingle_asset_id][..]) {
                return Err(template(
                    index,
                    what,
                    "the foreign assets are not Bingle$ alone",
                ));
            }
            Ok(())
        }
    }
}

/// Refuses a transaction carrying any of `fields` (name, present).
fn only_fields(
    index: usize,
    what: &'static str,
    fields: &[(&str, bool)],
) -> Result<(), RegistrationGroupError> {
    match fields.iter().find(|(_, present)| *present) {
        Some((name, _)) => Err(template(index, what, &format!("it has a {name} field"))),
        None => Ok(()),
    }
}

/// Refuses application-call fields on a transaction that isn't an application call.
fn no_app_fields(
    index: usize,
    what: &'static str,
    txn: &RawTransaction,
) -> Result<(), RegistrationGroupError> {
    only_fields(
        index,
        what,
        &[
            ("apid", txn.apid.is_some()),
            ("apan", txn.apan.is_some()),
            ("apaa", txn.apaa.is_some()),
            ("apat", txn.apat.is_some()),
            ("apfa", txn.apfa.is_some()),
            ("apas", txn.apas.is_some()),
        ],
    )
}

fn template(index: usize, expected: &'static str, reason: &str) -> RegistrationGroupError {
    RegistrationGroupError::Template {
        index,
        expected,
        reason: reason.to_string(),
    }
}

fn public_key(key: &SigningKey) -> Vec<u8> {
    key.verifying_key().to_bytes().to_vec()
}

/// Signs `"TX"` and the transaction bytes, and returns the signed transaction `{sig, txn}` as
/// base64 msgpack, with the transaction's bytes unchanged.
fn sign(bytes: &[u8], key: &SigningKey) -> String {
    let mut message = Vec::with_capacity(2 + bytes.len());
    message.extend_from_slice(b"TX");
    message.extend_from_slice(bytes);
    let signature = key.sign(&message).to_bytes();

    // A two-entry map with its keys in canonical (sorted) order: "sig" as 64 bytes of bin 8, then
    // "txn" as the original transaction map.
    let mut signed = Vec::with_capacity(4 + 3 + 2 + 64 + 4 + bytes.len());
    signed.push(0x82);
    signed.extend_from_slice(&[0xa3, b's', b'i', b'g', 0xc4, 64]);
    signed.extend_from_slice(&signature);
    signed.extend_from_slice(&[0xa3, b't', b'x', b'n']);
    signed.extend_from_slice(bytes);
    BASE64.encode(signed)
}
