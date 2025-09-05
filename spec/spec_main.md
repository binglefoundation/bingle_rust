Bingle Protocol Spec
====================

# Overview

![Overview](bingle_overview.svg "Overview")

Bingle is a protocol for secure, decentralised, distributed messaging.

Each Bingle node has an identifier (an Algorand address, which is based on an ed25519 public key)
and is associated with a unique user generated `handle`.

Typically, a user will send a message to a handle, and will receive messages addressed to their own handle.
A Bingle API provides this functionality to an application on a device.

Nodes and handles are registered on the Algorand blockchain. This also provides a TLS compatible PKI to facilitate secure end to end encrypted messaging.

Where a node is able to get a reasonably persistent IP address (this is driven by the NAT implemntation of the node's internet connection), the node can receive messages directly using a DTLS endpoint.
The IP address is advertised in a distributed database, allowing a node to determine the endpoint for an id.
Where a node is unable to get a persistent IP address, it can use a relay (based on TURN) to receive traffic. In this case the relay endpoint is advertised.
The protocol is extensible to support other transports, potentially TOR.

Registering a user into Bingle requires a small fee, paid in Algo (this is in addition to Algo transaction costs).

Users can opt to run a regular or a relay node. The latter provides relay functionality and the distributed DB.
There is a financial incentive to operate a relay node - based on availabiliuty and reliability, operators will be credited Algo.

# Interactions

## Handle lookup

This is initiated by a Bingle API call `handleLookup(handle) => id`.

The prerequisite for this is that the node has been initialized.
It will lookup the `handle` in the Algorand blockchain and return the associated `id`.
If multiple entries exist, we want the newest one.
If no entry is found, we want to return NULL/Fail.

Steps for this are as follows:

- get or create a blockchain (Algorand) connection
- search the local state for each account opted into the Bingle application
- filter this to entries where `AppTag` [change?] equals the `handle` parameter
- return the newest entry's `id` based on `AppTagTime`
- if no entry is found, return NULL/Fail

## Outgoing message transfer

This implements three versions:

When we have the `id`, and don't need a response, use `sendMessageToId(userId, message, progressCallback) => boolean`
When we have a `handle`, and don't need a response, use `sendMessageToHandle(handle, message, progressCallback) => boolean`
When we have both the endpoint `NetworkSourceKey` and the `id` use `sendMessageToNetwork(networkSourceKey, userId, message, progressCallback) => boolean`

When we have the id and need a response, use `sendMessageToIdWithResponse(userId, message, progressCallback) => Response`
When we have a handle and need a response, use `sendMessageToHandleWithResponse(handle, message, progressCallback) => Response`
When we have both the endpoint `NetworkSourceKey` and the `id` and need a response, use `sendMessageToNetworkWithResponse(networkSourceKey, userId, message, progressCallback) => Response`

The prerequisite for this is that the node has been initialized.

Steps are as follows:

- if we have a handle, use `handleLookup` to get the `id` _(sendMessageToHandle/sendMessageToHandleWithResponse)_
- if we don't have a `NetworkSourceKey`, look it up as follows: _(sendMessageToHandle/sendMessageToId/sendMessageToHandleWithResponse/sendMessageToIdWithResponse)_
  - (see Distributed Database section below)
  - find our relay to use
  - send a `DdbQueryResolve` message to the relay with our `id`
  - await the `DdbQueryResponse` message with the endpoint details in the `advertRecord`
- if we have a direct DTLS endpoint, use it to send our message over DTLS
- if we have a relay endpoint, send the message as described in the Relay DTLS section below
- if we need a response, wait for this (see Incoming message transfer)

At each stage, call the progress callback with the current progress (if one has been provided).

For versions requiring a response, the response is a `Response` command object, (see command reference below).

## Incoming message transfer

This is initiated by a message being received by direct DTLS or relayed DTLS.

The DTLS layer will have validated the `id` of the message sender and decrypted the message.

If the message has a `tag`, it will be matched to the `responseTag` sent on the request. If matched, the requests wait will finish and it will return the response message.

Otherwise, if the message is not a `PlainTextMessage`, it will be passed to a handler selected by the type value.

For a `PlainTextMessage` an `onMessage` callback in the API will be used to notify message arrival.

[Document any message handling not discussed below]

### Direct DTLS

Direct DTLS uses an on-demand DTLS connection.

This is expected to decide dynamically whether to send as a client (when no connection already exists) or a server (when an inbound connection has been established).

The DTLS protocol uses the Algorand blockchain to facilitate a serverless PKI.

Each `id` is a registered Algorand address which is opted in to the Bingle DAPP.

The CA certificate is specific to the source node and has the `id` stored in the issuer CN
(in the format <address>.ids.bingler.net [TODO])

The CA certificate is signed using the Algorand (ed25519) private key, ensuring that only the owner of the `id` can sign.

Server and client certificates use the `RSASSA-PSS` algorithm with a 2048 bit key and a SHA-512 hash.
They are signed by the CA private key. Their private keys are generated for each connection and deleted afterwards.

This process enables secure message delivery between DTLS endpoints with encryption and identity verification.

### Relay DTLS

## Initialisation and advertisement

## Registration

## Distributed database

# Message reference

!include ../generated/message_reference.md

# Blockchain interface
