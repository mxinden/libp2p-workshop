# libp2p-workshop

Hi there,

Welcome to the libp2p workshop.

We will build a peer-to-peer decentralized chat app using libp2p. Our
application will allow anyone with internet access across the globe to
communicate without depending on any central infrastructure. The workshop will
give hands-on experience on how to build peer-to-peer vs. client-to-server.

<!-- markdown-toc start - Don't edit this section. Run M-x markdown-toc-refresh-toc -->
**Table of Contents**

- [Before the workshop](#before-the-workshop)
- [Workshop](#workshop)
- [Iteration 1](#iteration-1)
- [Iteration 2](#iteration-2)
- [Additional Resources](#additional-resources)
    - [Libp2p](#libp2p)
    - [Rust Programming Language](#rust-programming-language)

<!-- markdown-toc end -->

## Before the workshop

1. Install git.

   https://git-scm.com/book/en/v2/Getting-Started-Installing-Git

2. Install Rust Programming Language.

   https://www.rust-lang.org/tools/install

3. Install Protoc, the Protobuf compiler.

   - https://github.com/protocolbuffers/protobuf#protocol-compiler-installation
   - Linux / MacOS: <https://grpc.io/docs/protoc-installation/>
   - Windows: <https://www.geeksforgeeks.org/how-to-install-protocol-buffers-on-windows/>

4. Clone this repository.

   ```sh
   $ git clone https://github.com/mxinden/libp2p-workshop.git
   ```

5. Make sure you can compile the _hello world_ program in this repository on the
   `main` branch.

   ```sh
   $ cargo run

   Finished dev [unoptimized + debuginfo] target(s) in 0.04s
   Running `target/debug/libp2p-workshop-node`
   Hello, world!
   ```

Done? Great. You are all set for the workshop.

## Workshop

Let's start with the [first iteration](
https://github.com/mxinden/libp2p-workshop/blob/iteration-1/README.md#iteration-1).

## Iteration 1

In case you are still on branch `main` switch over to branch `iteration-1`.

```
$ git checkout iteration-1
```

In this iteration, we will have our node implementation connect to a bootstrap
node. More particularly we will connect to the node below:

```
/ip4/18.237.216.248/tcp/7654/p2p/12D3KooWSrPEpy6z9gbvxWhCQYTKmZcpkwTUyUDtoF2KzcrC4y5K
```

For those interested, the above is a
[multiaddr](https://github.com/multiformats/multiaddr). Composable and
future-proof network addresses.

To connect to the bootstrap node, run the command below:

```
$ cargo run -- --bootstrap-node /ip4/18.237.216.248/tcp/7654/p2p/12D3KooWSrPEpy6z9gbvxWhCQYTKmZcpkwTUyUDtoF2KzcrC4y5K

Local peer id: PeerId("12D3KooWQ7XeB9dgLZniYZ7nypcHYwEDyGe9eRbkDDmhB9upurMc")
Connected to TODO.
```

Given that both nodes run the [Ping
protocol](https://docs.rs/libp2p-ping/latest/libp2p_ping/), they exchange
Ping-Pong style messages. You should see the results of these message exchanges
printed as logs along with the round-trip-time (RTT).

Let's move on to [iteration two](
https://github.com/mxinden/libp2p-workshop/blob/iteration-2/README.md#iteration-2).

## Iteration 2

In case you are still on branch `iteration-1` switch over to branch `iteration-2`.

```
$ git checkout iteration-2
``` 

Compared to the previous iteration, the only change in this iteration is that we
are introducing the [identify
protocol](https://docs.rs/libp2p/latest/libp2p/identify/index.html). It is a
simple protocol allowing two nodes to exchange basic information like listening
addresses and supported protocols.


```
$ cargo run -- --bootstrap-node /ip4/18.237.216.248/tcp/7654/p2p/12D3KooWSrPEpy6z9gbvxWhCQYTKmZcpkwTUyUDtoF2KzcrC4y5K

[2022-10-28T19:26:44Z INFO  libp2p_workshop_node] Local peer id: PeerId("12D3KooWGDU8Ngs46ekukV6aMvGcX8zVTs2FSKun6PjZeJMBAyXJ")
[2022-10-28T19:26:44Z INFO  libp2p_workshop_node] Listening on /ip4/127.0.0.1/tcp/41783.
[2022-10-28T19:26:44Z INFO  libp2p_workshop_node] Listening on /ip4/192.168.17.85/tcp/41783.
[2022-10-28T19:26:46Z INFO  libp2p_workshop_node] Connected to /ip4/18.237.216.248/tcp/7654/p2p/12D3KooWSrPEpy6z9gbvxWhCQYTKmZcpkwTUyUDtoF2KzcrC4y5K.
[2022-10-28T19:26:46Z INFO  libp2p_workshop_node] Received Identify Info
    Peer: 12D3KooWSrPEpy6z9gbvxWhCQYTKmZcpkwTUyUDtoF2KzcrC4y5K, Agent version github.com/marcopolo/public-ipfs/workshop-server/m/v2@
```

Let's move on to [iteration three](
https://github.com/mxinden/libp2p-workshop/blob/iteration-3/README.md#iteration-3).

## Additional Resources

Below are a couple of resources for those interested in reading more about
the stack used in this workshop.
**No knowledge is required in order to participate in the workshop!**

### Libp2p

Libp2p is a modular network stack that enables the development of peer-to-peer network applications.

- Introduction to Libp2p: <https://docs.libp2p.io/introduction/>
- Tutorial for getting started with rust-libp2p: <https://github.com/libp2p/rust-libp2p/blob/master/src/tutorials/ping.rs>
- Libp2p Specs: <https://github.com/libp2p/specs>

### Rust Programming Language

In this workshop we are using the Rust implementation of the libp2p networking stack.

- Rust Getting started: <https://www.rust-lang.org/learn/get-started>
- The Rust Book: <https://doc.rust-lang.org/stable/book/>
