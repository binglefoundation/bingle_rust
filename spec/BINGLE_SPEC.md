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

## Message transfer

### Direct DTLS

### Relay DTLS

## Initialisation and advertisement

## Registration

## Distributed database

# Message reference

# Blockchain interface
