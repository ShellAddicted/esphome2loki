# esphome2loki
Just a bridge between [ESPHome](https://esphome.io/) devices' logs (MQTT) and [Grafana Loki](https://grafana.com/oss/loki/)

#  Deployment
*Note:* using the 'latest' tag is not recommended, you should pick a version.
## 1. docker-compose (recommended)
```yaml
esphomelogs2loki:
    image: ghcr.io/shelladdicted/esphome2loki:latest
    restart: unless-stopped
    volumes:
        - ./data/config.toml:/config/config.toml
```
## 2. Docker
```bash
docker run -d \
   --name=esphome2loki \
   --restart=unless-stopped \
   -v <path/to/config.toml>:/config/config.toml \
   ghcr.io/shelladdicted/esphome2loki:latest
```
## 3. Build from source
```bash
git clone https://github.com/ShellAddicted/esphome2loki
cd esphome2loki
cargo build --release
cp sample_config.toml config.toml # and edit config.toml
./target/release/esphome2loki # Run it 
```