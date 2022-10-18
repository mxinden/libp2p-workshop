# libp2p-workshop

Hi there,

Welcome to the libp2p workshop.

We will build a peer-to-peer decentralized chat app using libp2p. Our
application will allow anyone with internet access across the globe to
communicate without depending on any central infrastructure. The workshop will
give hands-on experience on how to build peer-to-peer vs. client-to-server.

## Before the workshop

1. Install git.

   https://git-scm.com/book/en/v2/Getting-Started-Installing-Git

2. Install Rust Programming Language.

   https://www.rust-lang.org/tools/install

3. Install Protoc, the Protobuf compiler.

   https://github.com/protocolbuffers/protobuf#protocol-compiler-installation

4. Clone this repository.

   ```
   $ git clone https://github.com/mxinden/libp2p-workshop.git
   ```

5. Make sure you can compile the _hello world_ program in this repository on the
   `main` branch.

   ```
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

TODO: Add ping, keep alive

In case you are still on branch `main` switch over to branch `iteration-1`.

```
$ git checkout iteration-1
```

In this iteration, we will have our node implementation connect to a bootstrap
node. More particularly we will connect to the node below:

```
TODO: Add address here
```

TODO: Reference Multiadddr

To connect, run the command below:

```
$ cargo run -- --bootstrap-node TODO

Local peer id: PeerId("12D3KooWQ7XeB9dgLZniYZ7nypcHYwEDyGe9eRbkDDmhB9upurMc")
Connected to TODO.
```

## Iteration 2

TODO: Identify, remove ping

## Iteration 3

TODO: Gossipsub

## Iteration 4

TODO: Gossip addresses and try to connect to them
