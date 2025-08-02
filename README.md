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
    - [ ] Through Docker and Kubernetes
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

## Hosting
#### Requirements
* A Linux-based server (currently, Windows should work, but it will not be supported in the future)
* A domain (example.com) pointing to the server (can be proxied by services like cloudflare)
* a wildcard domain (*.example.com) pointing to the server (can _not_ be proxied by services like cloudflare)

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
    /database.db  # sqlite database if using sqlite
    /icons        # world, user and mod icons
    /versions     # server .jar files
    /worlds       # server files, if running locally
/infrarust
    /infrarust    # the infrarust executable, this will be embedded in later versions
config.toml       # the configuration file
```

#### Frontend

to run the fontend, you will need to have `NodeJS` installed, along with a package manager (`npm`, `pnpm`, or whatever you prefer). After you install it, in the repository folder run

```
cd mcmanager-frontend
npm i
npm run build
```
this will generate the static page files in the `dist` subfolder, which can later be served to the client

#### HTTP Proxy

to actually serve this project, you will need a reverse proxy. i use nginx, so i recommend it, but any reverse proxy with static file serving and websocket functionality will do. you need to configure it to do the following:

* proxy `/api` to the backend
* proxy `/socket.io` to the backend
* serve the frontend's `dist` directory like you would a normal Vue single-page app. you can read more about it [here](https://cli.vuejs.org/guide/deployment.html)

example nginx configuration
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

## Configuration
before you start using MCManager, you might want to tweak the configuration. primarily, you have to **set the correct domain** in the config file, otherwise the minecraft proxy will not work. 
## Project Structure
```
/mcmanager-frontend # frontend
/src
  /api              # API and Socket.IO handlers.
  /bin              # contains the (currently broken) minimanager binary
  /database         # objects and data types stored in the database
  /minecraft        # minecraft server handling
  /resources
    /config         # default configs
    /icons          # default icons
  api.rs            # the API server
  config.rs         # config handling
  database.rs       # database handling and abstraction
  main.rs           # the main executable
```
