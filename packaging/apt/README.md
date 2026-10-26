# MBL Signed APT Repository (`packaging/apt`, M5.12)

Provides the APT repository generator (`build-apt-repo.sh`) and client configuration (`mbl.list`) so Raspberry Pi OS (`arm64`) and Debian/Ubuntu (`amd64`/`arm64`) installations receive updates via standard `apt update && apt upgrade`.

## Client Setup
```bash
curl -fsSL https://msorenss.github.io/mbl/apt/mbl-archive-keyring.gpg \
  | sudo tee /usr/share/keyrings/mbl-archive-keyring.gpg > /dev/null
sudo cp packaging/apt/mbl.list /etc/apt/sources.list.d/mbl.list
sudo apt update && sudo apt install mbl
```
