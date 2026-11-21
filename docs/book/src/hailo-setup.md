# Setting up a Hailo NPU

A Hailo NPU lets MBL look at the picture and listen to speech without sending anything out of
the house. This page takes you from a bare board to working scene descriptions and speech
recognition. Most of it is a one-off; the models themselves are installed from MBL.

## What you get

| Board | Raspberry Pi product | What MBL does with it |
|---|---|---|
| **Hailo-10H** | AI HAT+ 2 | Objects and people in the picture (YOLO), a sentence describing the scene (Qwen-VL), speech to text for **Talk** (Whisper) |
| **Hailo-8 / 8L** | AI HAT+, AI Kit | Objects and people in the picture (YOLO). No scene sentences or speech: those models need the Hailo-10H's own memory |
| None | | The companion still works from dialogue and metadata; the "picture" is only a rough colour guess, marked as such |

On Windows or a PC without a Hailo, MBL has no NPU path yet. The assistant itself can still run
on the graphics card through a local model server such as Ollama, see
[AI Watch Companion](ai-companion.md).

## 1. Fit the board

Fit the HAT on the Raspberry Pi 5 and make sure PCIe is on (Raspberry Pi OS does this for the
AI HATs). After a reboot:

```bash
lspci | grep -i hailo
```

A Hailo-10H appears as `/dev/h1x-0`, a Hailo-8 as `/dev/hailo0`.

## 2. Install the driver and HailoRT

The driver, the HailoRT library and everything that uses the board (MBL's models, Frigate)
must be **the same HailoRT version**. Mixing versions is the most common cause of trouble.

**Hailo-10H**, from the Raspberry Pi repository:

```bash
sudo apt update
```

```bash
sudo apt install hailo-h10-all
```

That installs the HailoRT version Raspberry Pi has packaged (5.1 at the time of writing). For a
newer one, download the `h10-hailort-pcie-driver` and `h10-hailort` packages of the same version
from Hailo's Developer Zone (free account) and install both with `sudo apt install ./*.deb`.

**Hailo-8 / 8L:**

```bash
sudo apt install hailo-all
```

Then reboot and check that HailoRT sees the board:

```bash
hailortcli --version
```

```bash
hailortcli fw-control identify
```

MBL builds a small bridge to HailoRT the first time it uses the NPU, so it needs a C++ compiler:

```bash
sudo apt install g++
```

## 3. Install the models from MBL

Open **Administration → Hardware → AI models for the NPU**. MBL shows the NPU, the HailoRT
version and the models it uses:

- **Qwen2-VL 2B** (3.3 GB): describes the scene in a sentence for the companion and the assistant.
- **Whisper Base** (130 MB): speech to text for **Talk** in the chats.

**Download** fetches the build that matches your HailoRT version from Hailo's server
(`dev-public.hailo.ai/v<version>/…`), checks that HailoRT can read it and puts it in
`/var/lib/mbl/models`. No restart is needed. **In use** shows which file MBL picks for each job.
Pick one model per job; the alternatives (Qwen3-VL, Whisper Small) are there if you want to try
them.

Object detection uses YOLO. If Frigate is installed, MBL uses Frigate's compiled YOLO models from
its `model_cache`; otherwise pick a `.hef` with **Choose .hef** on the same page.

## 4. Check that it works

- **Hardware** shows the NPU's temperature and load, and the self-test includes an NPU run.
- In the player chat, turn on **Screenshots to the AI** (or just ask the built-in companion what
  is going on): the context then includes what is on screen. The first scene sentence takes
  about 45 seconds while the model loads; after that 6–11 seconds per frame on a Raspberry Pi 5.
- **Talk** in a chat should say "Speech is recognized on this server" in its tooltip.

## Sharing the board with Frigate

MBL and Frigate can use the NPU at the same time: MBL joins HailoRT's shared device and runs at a
lower priority, so Frigate's detections are not delayed. Keep Frigate's image on the same HailoRT
version as the system. The Hailo-10H has 8 GB of its own memory; Frigate's YOLO, the scene model
and Whisper fit together, a large language model as well usually does not.

## Upgrading HailoRT

Upgrade everything together, in this order:

1. Stop Frigate and MBL: `sudo systemctl stop mbl`.
2. Install the new driver and HailoRT packages, then reboot.
3. Update Frigate's image to the same HailoRT version.
4. In MBL, **Remove** the old models and **Download** them again: models are built for one
   HailoRT version.
5. Start MBL again.

## Troubleshooting

| What you see | What to do |
|---|---|
| "No Hailo NPU was found" | Check `lspci` and the device file from step 1; reboot after installing the driver |
| "HailoRT is not installed" | Install the packages in step 2; `hailortcli --version` must work |
| "These models need a Hailo-10H" | A Hailo-8 runs detection only; the scene sentence and speech need a Hailo-10H |
| Download says Hailo's server has no file for your version | Hailo publishes models for selected versions; install a HailoRT version that has them |
| Scene sentences never appear | Check that the model shows **In use**, that `g++` is installed, and the MBL log (`journalctl -u mbl`) |
| Frigate stops detecting after an upgrade | Frigate's image uses another HailoRT version than the driver; update the image |
