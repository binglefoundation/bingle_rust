# ping_process

## Create message classes

- Create (using OpenAPI 3) handlers for each message class defined in spec/openapi.yaml
- Create a marshaller that decodes JSON messages into message types
- Create a message router that calls a handler function for each message type, leaving all unimplemented
- Create an unimplemented message handler that prints the message

## Implement an Engine class.

- This is initialized with config from StartOptions and then performs the following startup processing:

- if the staticEndpoint is set:
-- create a NetworkMuxUdp instance
-- create a DtlsOpenSsl instance 
-- add a handle_message to the DtlsOpenSsl instance
--- decode the JSON and call the message router to select a handler
-- start this DtlsOpenSsl instance, passing in the NetworkMuxUdp instance
-- start the NetworkMuxUdp instance
- else
- if the staticEndpoint is not set, raise a non implemented error

## Implement relay ping handling

- Implement a handler for RelayTriangleTest1
-- If we have a peer relay node, send it a RelayTriangleTest2 message

- Implement a handler for RelayTriangleTest2
-- Send a RelayTriangleTest3 message to the node at `CheckingEndpoint`

# dtls_pki

## Add issuer information to DTLS

- Add issuer string to dtls_trait.send
- Add issuer string to HandleMessage

## Initialize our certificates
 
- Initialize an AlgoOps instance from a passphrase in StartOptions "algoPassphrase" (add this field)
- Obtain our id (address) from the AlgoOps and use this as issuer by appending .ids.bingle.home.arpa to it
- Generate a CA certificate using ed25519 using the Algorand private key
- Use RSAPSS algorithm with a 2048 bit key and a SHA-512 hash for server and client certs
- Generate an ephemeral server certificate and private key and sign with CA cert
- Generate an ephemeral client certificate and private key and sign with CA cert

## Create a HandlePeerCertificate

- Extract the CA certificates id (Algo public key) from its issuer by removing the trailing .ids.bingle.home.arpa
- Validate that the CA certificate is signed by the Algo public key
- Extract the server/client certificate id (Algo public key) from its issuer by removing the trailing .ids.bingle.home.arpa
- Validate that the server/client certificate is signed by the CA certificate
- Validate that the server/client certificate has a valid issuer which matches the AC cert
- If everything valid return the issuer
- If not return an Error

## Associate issuer with socket endpoint

- create a map of endpoint to issuer
- When HandlePeerCertificate is called, associate the issuer with the socket endpoint
- On any DTLS error, clear the issuer from the map
- When a message is received, look up the issuer and include it in the message
- When a message is sent, validate that the endpoint and issuer match

# relay_finder

## Implement message tagging

- Implement `send_message_to_network_with_response`
-- Create a tag as a random UUID
-- Store this in a map of tag UUID to a structure with a signal primitive and a response message
-- Add the field `responseTag: <tag>` to the message
-- Split into two threads
-- In one thread, wait sychronously to be signalled via the signal primitive 
-- In the other thread, send the message and end the thread
-- Once signalled, remove the tag from the map and return the message
-- If a timeout occurs, remove the tag from the map and return an error

- In the message handler, if we have a `tag` in the message look it up in the above map
- Discard the message and log an error if not found
- Otherwise, populate the received message field
- and signal the signalling primitive

(Choose an appropriate signalling primitive)

## Implement RelayCheck

- Implement a handler for RelayCheck as specified
- In this handler, put the `responseTag` in the `tag` field of the `RelayCheckResponse` and send it

## Create a RelayFinder for root relays

- Create a RelayFinder class
- Implement this as described in the spec "Finding relay servers" for root relays only
- Once RelayCheck has been called, cache and return the root relay to use
- Return from further calls with the cached value until it times out

# endpoint_identify

- Implement the path in Engine startup when staticEndpoint is not set
- Set the initial engine state to StunIdentify
- Create an EndpointFinder and associate this with our NetworkMuxUDP to handle STUN messages. Pass the StunServers config field
- When the STUN stateChangeHandler is called with Consistent
-- set the engine state to TrianglePing
-- find our peer relay node
-- send a RelayTriangleTest1 message to our peer relay node
-- await an inbound RelayTriangleTest3 message
-- If we receive a RelayTriangleTest3 message, set the engine state to EndpointAvailable
- When the STUN stateChangeHandler is called with Stale
- If we don't receive a message in 10 seconds, raise a non implemented error
- When the STUN stateChangeHandler is called with Inconsistent
-- raise a non implemented error