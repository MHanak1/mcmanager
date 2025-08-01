<p align="center">
    <img alt="MCManager" src="https://raw.githubusercontent.com/MHanak1/mcmanager/refs/heads/master/src/resources/icons/logo.png"/>
</p>
MCManager is a Minecraft server manager written in Rust with support for small and large deployments alike.

## Why? 

MCManager is a self-hosted alternative to services like Aternos or Minehut. It's meant to provide their convenience, while giving the control of a self-hosted solution. 

## How?

The MCManager uses [Infrarust](https://infrarust.dev/) to proxy traffic to users' servers, based on the hostname. Those servers can be run in several ways:
* **Internaly** - The Minecraft servers are run as the API server's subprocesses. while this requires almost no setup, it's not very scalable and not at all secure. This should only be used when all users can be trusted.
* **Remotely** (NOT IMPLEMENTEED) - The Minecraft servers can be launched on a separate machine. this slightly more secure, but not much different than internal servers.
* **Containerized** (NOT IMPLEMENTED) - The Minecraft servers can be launched on Kubernetes containers separate for each user. This is the best (if not only) option for larger deployments. It is both scalable, and generally secure.

## Roadmap

- [x] Data storage and API authentication
- [x] Run servers
    - [x] Locally
    - [ ] Remotely
    - [ ] Through Kubernetes
- [x] Proxy traffic through Infrarust
- [x] A web frontend
- [x] Websocket server console
- [ ] Forge and NeoForge compatibility
- [ ] Mod and plugin support
- [ ] Modrinth integration

## Compatibility

| Vanilla | Paper | Fabric | Quilt | Forge | NeoForge | Bukkit | Spigot |
|---------|-------|--------|-------|-------|----------|--------|--------|
| ✅       | ?     | ✅      | ?     | ❌     | ❌        | ?      | ?      |

#### Versions

MCManager should in theory be compatible with all versions from 1.7.10 onwards, but at the moment the older versions don't appear to work

\* At this point the project is *not* ready for a large-scale deployment
