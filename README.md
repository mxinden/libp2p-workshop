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

2. Install Rust.

   https://www.rust-lang.org/learn/get-started

3. Install Protoc, the Protobuf compiler.

   https://github.com/protocolbuffers/protobuf#protocol-compiler-installation

4. Make sure you can compile the _hello world_ program in this repository on this branch.

   ```
   $ cargo run

   Finished dev [unoptimized + debuginfo] target(s) in 0.04s
   Running `target/debug/libp2p-workshop-node`
   Hello, world!
   ```

Done? Great. Let's move on then to the [first iteration](
https://github.com/mxinden/libp2p-workshop/blob/iteration-1/README.md#iteration-1).

## Iteration 1

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
