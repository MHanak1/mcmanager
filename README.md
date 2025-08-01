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

## Hosting

#### Backend

the project is still in early development, so the installation process is very manual. furthermore, at this point, the project is still evolving very quickly, so i do not publish releases. In order to run MCManager you will have to compile it yourself. The first step to do so is to you will need to install cargo. You can do so by following the steps [here](https://doc.rust-lang.org/cargo/getting-started/installation.html). after you do so, you can compile MCManager with these commands:

```bash
git clone https://github.com/MHanak1/mcmanager.git # or if you don't have git you can download the repository manually
cd mcmanager
cargo build --release --bin mcmanager
cp ./target/release/mcmanager ./
```
after you run those, you should be able to run the `mcmanager` executable. after prompting for the username and password for the administrator account it should generate several files and folders in the working directory:
```
/data
    /database.db # sqlite database if using sqlite
    /icons # world, user and mod icons
    /versions # server .jar files
    /worlds # server files, if running locally
/infrarust
    /infrarust # the infrarust executable, this will be embedded in later versions
config.toml # the configuration file. for more detail see https://github.com/MHanak1/mcmanager/blob/master/src/resources/configs/default_config.toml
```

#### Frontend
i don't have much time to write, so install npm or pnpm, go to the `mcmanager/mcmanager-frontend` folder, and run
```
[p]npm run build
```
and this will generate the static html in the `dist` subfolder
#### HTTP Proxy

example nginx configuration (todo: write things here i dunno)
```conf
server {
    listen       80;
    server_name  example.com;

    location /api {
        proxy_set_header    Host                $host;
        proxy_set_header    X-Real-IP           $remote_addr;
        proxy_set_header    X-Forwarded-For $proxy_add_x_forwarded_for;

        proxy_pass         http://127.0.0.1:3030/api;
    }

    location /socket.io {
        proxy_http_version  1.1;
        proxy_set_header    Upgrade             $http_upgrade;
        proxy_set_header    Host                $host;
        proxy_set_header    X-Real-IP           $remote_addr;
        proxy_set_header    X-Forwarded-For $proxy_add_x_forwarded_for;

        proxy_pass         http://127.0.0.1:3030/socket.io;
    }

    location / {
       root   /PATH/TO/MCMANAGER/mcmanager-frontend/dist;
       index  index.html;
       try_files $uri $uri/ /index.html;
    }
    error_page   500 502 503 504  /50x.html;
    location = /50x.html {
        root   /usr/share/nginx/html;
    }
}
```
