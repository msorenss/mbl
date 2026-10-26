# Docker & Container Compose

MBL publishes multi-architecture container images (`ghcr.io/msorenss/mbl:v1.0.0`) supporting both `linux/amd64` and `linux/arm64`.

## Basic `docker-compose.yml`

```yaml
services:
  mbl:
    image: ghcr.io/msorenss/mbl:latest
    container_name: mbl
    restart: unless-stopped
    ports:
      - "8420:8420"
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Europe/Stockholm
    volumes:
      - ./config:/config
      - /mnt/storage/movies:/media/movies:ro
      - /mnt/storage/tv:/media/tv:ro
      - /dev/shm:/tmp/transcode
```

## Intel QSV & VAAPI Hardware Acceleration

Pass the DRM render node into the container:

```yaml
    devices:
      - /dev/dri:/dev/dri
    group_add:
      - "44" # Video/render group ID on host
```

## NVIDIA NVENC Hardware Acceleration

Using NVIDIA Container Toolkit:

```yaml
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu, video]
```
