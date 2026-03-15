# Bingle Webserver

This crate implements a local HTTP server that acts as a bridge between a browser client and the Bingle API.

## Building

To build the webserver:

```bash
cargo build -p bingle_webserver
```

## Running

To run the webserver with default settings (port 12121, address 127.0.0.1):

```bash
cargo run -p bingle_webserver -- <handle>
```

You can specify the port and address:

```bash
cargo run -p bingle_webserver -- --port 8080 --address 0.0.0.0 <handle>
```

Other arguments are forwarded to the Bingle API, same as `bingle_cli run`.

### Running against Testnet

To run against the Algorand testnet using the provided Nodely configuration:

```bash
cargo run -p bingle_webserver -- <handle> --node-file nodely_testnet_node.json --passphrase "your passphrase"
```

## Testing with curl

Once the server is running, you can interact with it using `curl`.

### Lookup an ID by handle

```bash
curl "http://localhost:12121/handleLookup?handle=alice"
```

### Send a plaintext message to a User ID

```bash
curl -X POST http://localhost:12121/sendMessageToId \
     -H "Content-Type: application/json" \
     -d '{
       "userId": "P577...",
       "message": { "text": "Hello from curl" }
     }'
```

### Send a message to a handle

```bash
curl -X POST http://localhost:12121/sendMessageToHandle \
     -H "Content-Type: application/json" \
     -d '{
       "handle": "bob",
       "message": { "text": "Hi Bob" }
     }'
```

### Retrieve queued messages

```bash
curl http://localhost:12121/queued
```

### Send a message to a network endpoint

```bash
curl -X POST http://localhost:12121/sendMessageToNetwork \
     -H "Content-Type: application/json" \
     -d '{
       "networkSourceKey": {
         "inetSocketAddress": {
           "host": "127.0.0.1",
           "port": 4433
         }
       },
       "userId": "P577...",
       "message": { "text": "Direct message" }
     }'
```
