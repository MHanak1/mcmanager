<p align="center">
    <img alt="MCManager" src="https://raw.githubusercontent.com/MHanak1/mcmanager/refs/heads/master/src/resources/logo.png"/>
</p>
MCManager is a Minecraft server manager with support for small and large deployments alike.

## Why? 

The purpose of this project is to let anyone spin up a new Minecraft world in the matter of seconds, 
whether it's a one-user instance, or a large distributed deployment with hundreds of people*.

## How?

The MCManager uses [Velocity](https://papermc.io/software/velocity) to proxy traffic to users' servers. Those servers can be run in several ways:
* **Internaly** - The Minecraft servers are run as the API server's subprocesses. while this requires almost no setup, it's not very scalable and not at all secure. This should only be used when all users can be trusted.
* **Remotely** - The Minecraft servers can be launched on a separate machine. this slightly more secure, but not much different than internal servers.
* through **Kubernetes** - The Minecraft servers can be launched on Kubernetes containers separate to each user. This is the best (if not only) option for larger deployments. It is both scalable, and generally secure.

## Roadmap

#### Backend
- [x] Data storage and authentication with an API
- [x] Run servers
    - [x] Locally
    - [x] Remotely
    - [ ] Through Kubernetes
- [x] Proxy traffic through Velocity
- [ ] Handling uploads of mods and versions
- [ ] Automatically download mods and versions from modrinth

#### Frontend
- [ ] Frontend


\* At this point the project is *not* ready for a large-scale deployment