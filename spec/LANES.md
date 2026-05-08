api_server:
- ~~Implement api wrapper for lookupHandle and test~~
- ~~Implement api wrapper for send to handle and test~~
- ~~Put handle info into messages, with handle cache~~

deployment:
- ~~add staging environment~~
- ~~deploy relays to AWS using CloudFormation~~
- ~~smoke test in staging~~

local user:
- ~~API to hold local user key material securely (portable initially)~~
- ~~API to hold contacts~~
- ~~Local user API in web server~~

UX:
- ~~build react UX with onboarding~~
- ~~JSI / Uniffi library interface~~
- ~~build react native UX based on messenger / signal / apple messages~~
- ~~user onboarding with held algorand~~
- contact UX
- build iOS APK
- tidy up bugs

security:
- ensure protocol is built as documented
- ensure DAPP methods perform all required checks
- delegate admin tasks to not be creator
- ensure runs in live with full encryption
- ensure endpoint is checked and fails on impersonation

robustness:
- clean up duplicated code
- ensure fails result in a fail message which gets handled
- implement retry for packet loss and retryable fails
- implement relay cache properly with expiry
- lookup matches handles with downcase and some punctuation normalized (as Gmail?)
- 
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
