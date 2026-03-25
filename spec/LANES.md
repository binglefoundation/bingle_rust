api_server:
- ~~Implement api wrapper for lookupHandle and test~~
- ~~Implement api wrapper for send to handle and test~~
- Put handle info into messages, with handle cache

deployment:
- ~~add staging environment~~
- deploy relays to AWS using CloudFormation
- smoke test in staging

local user:
- API to hold local user key material securely (portable initially)
- API to hold contacts

UX:
- build react native UX based on messenger / signal / apple messages
- user onboarding with held algorand
- contact UX
- build iOS APK

security:
- ensure protocol is built as documented
- ensure DAPP methods perform all required checks
- delegate admin tasks to not be creator
- ensure runs in live with full encryption
- ensure endpoint is checked and fails on impersonation

network:
- remove DDB entries on node stop
- handle clean relay shutdown
- cache DDB locally with timeout / cancel
- cache blockchain locally
- detect relay unresponsive from peers and remove
- handle network change and clear caches / rediscover nat type

tokenomics:
- develop tokenomics model
- implement pricing accordingly

android:
- support NDK in library
- support Android for react native UX
- build APK
