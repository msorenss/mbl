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

With HailoRT 5 the Hailo-10H runs its own server on the board, and every program on the Pi
(Frigate, MBL, `hailortcli`) connects to it as a client. There is no separate sharing service any
more: an old `hailort.service` that starts `hailort_service` belongs to HailoRT 4 and only fails
(see troubleshooting). Each connection uses a pair of the board's DMA channels, and there are
only so many, so programs that hold connections they do not use can lock others out.

MBL is careful about this:

- It connects only when a model is actually needed, with one shared connection per model for
  the whole server.
- It lets go of the detection model after 2 minutes without use, and of the scene description
  model after 15 minutes. Loading the description model takes about 45 seconds, and while it
  loads Frigate's detections run slower (in a test, about 40 ms became up to 180 ms for that
  time, then back to normal). So MBL loads it once per viewing: as soon as a companion joins the
  session, not in the middle of a conversation, and keeps it while the companion uses it.
- Running at the same time is fine: in the same test Frigate detected at its usual speed while
  MBL described scenes and transcribed speech.
- **Let MBL use the NPU** on the Hardware page turns it off completely; MBL then releases
  everything at once and the companion works from dialogue and metadata only. Turn it off if
  Frigate needs the board to itself.
- The Hardware page shows what MBL has loaded right now, how many frames, descriptions and
  transcriptions it has done since it started and the last problem, so you can see whether the
  NPU is doing anything useful.

Keep Frigate's image on the same HailoRT version as the system. The Hailo-10H has 8 GB of its own
memory; Frigate's YOLO, the scene model and Whisper fit together, a large language model as well
usually does not.

## Language models on the NPU (Hailo-Ollama)

On a Hailo-10H the built-in assistant can run on the board too, without a cloud service.
Hailo-Ollama is Hailo's small model server: it speaks the same language as Ollama (and the
OpenAI-style `/v1/chat/completions`), but runs models compiled for the Hailo-10H. The models are
small (1–2 billion parameters): good for short answers and summaries, weaker at long
conversations and at searching the library with tools. Try it; if the answers are not good
enough, a cloud provider or Ollama on a PC is the better assistant.

Hailo-8 / 8L boards cannot run these models.

### Install Hailo-Ollama

Hailo-Ollama comes in the package `hailo-gen-ai-model-zoo`, which must have **the same version as
HailoRT** (`hailortcli --version`). Check what you have:

```bash
dpkg -l | grep -E "hailort|h10-hailort|hailo-gen-ai"
```

All lines must show the same version, for example `5.4.0`.

**If you installed HailoRT from Raspberry Pi's repository** (`hailo-h10-all`), install the model
zoo from there too:

```bash
sudo apt install hailo-gen-ai-model-zoo
```

**If you installed HailoRT yourself from Hailo's Developer Zone** (a newer version than Raspberry
Pi packages, as for Frigate's `hailo-frigate-h10` image), apt cannot help: its version would not
match. Download the model zoo from the same place instead:

1. Sign in at Hailo's Developer Zone, open the software downloads and filter on the Hailo-10H,
   your HailoRT version and `arm64` (for a Raspberry Pi 5). The package is listed as the GenAI
   model zoo.
2. Download `hailo-gen-ai-model-zoo_<version>_arm64.deb` (for example
   `hailo-gen-ai-model-zoo_5.4.0_arm64.deb`).
3. Install it from the folder you downloaded it to:

```bash
sudo apt install ./hailo-gen-ai-model-zoo_5.4.0_arm64.deb
```

4. Stop apt from replacing it with Raspberry Pi's older version on the next upgrade:

```bash
sudo apt-mark hold hailo-gen-ai-model-zoo hailort h10-hailort-pcie-driver
```

(Use `sudo apt-mark unhold …` when you upgrade on purpose, see below.)

Check that the program is there:

```bash
hailo-ollama --help
```

The package installs `/usr/bin/hailo-ollama` and a list of the models Hailo offers in
`/usr/share/hailo-ollama/models/manifests`. It does not install a service and does not download
any models.

### Start it from MBL

Open **Administration → Hardware → Language models on the NPU**. With the package installed MBL
shows **Stopped** and a **Start** button. **Start**:

- runs `hailo-ollama` as the `mbl` user on `127.0.0.1:8000` (only reachable from the Pi itself),
- shares the board with Frigate and MBL's own models (`HAILO_OLLAMA_VDEVICE_GROUP_ID=SHARED`),
- keeps its downloaded models under `/var/lib/mbl/hailo-ollama`, next to MBL's database, so they
  are not mixed with anything else on the Pi,
- starts it again when MBL restarts, until you press **Stop**.

Turning off **Let MBL use the NPU** stops Hailo-Ollama too.

Then **Download** a model. Each is about 1.5–2.5 GB and comes from Hailo's server; MBL shows the
progress. A good first choice is `qwen2.5:1.5b`; `qwen3:1.7b` answers a little better and more
slowly, `llama3.2:1b` is the fastest. **Remove** deletes a model again.

### Use it for the assistant

Open **Administration → Server settings → Assistant**:

1. **Provider:** Hailo-Ollama.
2. **Address:** the one the Hardware page shows, normally `http://127.0.0.1:8000/v1`.
3. **Model:** **Fetch models** and pick the one you downloaded, for example `qwen2.5:1.5b`.
4. **Test connection**, then turn the assistant on.

The first answer after a pause takes longer while the model loads onto the board (tens of
seconds); after that answers start in a few seconds.

If the test says the model cannot use tools, the assistant can still chat but cannot look things
up in the library for you; a larger model elsewhere can.

### If you already run Hailo-Ollama yourself

MBL starts nothing if something already answers on `127.0.0.1:8000`: it uses that one, shows it
as **Running** and has no **Stop** button for it. That is the way to go if you want your own
settings, for example a systemd service:

```ini
# /etc/systemd/system/hailo-ollama.service
[Unit]
Description=Hailo-Ollama
After=network.target

[Service]
User=mbl
Environment=OLLAMA_HOST=127.0.0.1:8000
Environment=HAILO_OLLAMA_VDEVICE_GROUP_ID=SHARED
Environment=HOME=/var/lib/mbl/hailo-ollama
WorkingDirectory=/var/lib/mbl/hailo-ollama
ExecStart=/usr/bin/hailo-ollama
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
```

```bash
sudo systemctl enable --now hailo-ollama
```

Keep `127.0.0.1:8000`: MBL looks there. `HAILO_OLLAMA_VDEVICE_GROUP_ID=SHARED` is what lets it run
next to Frigate; leave it out and Hailo-Ollama may take the board for itself.

### Next to an ordinary Ollama

An ordinary Ollama (for example in Docker on port `11434`) does not use the Hailo board and does
not get in the way: the two use different ports. In the assistant settings, the **Ollama** preset
points to `11434` and runs on the processor, the **Hailo-Ollama** preset to `8000` and runs on the
NPU. Pick one.

### Memory on the board

The Hailo-10H has 8 GB of its own memory. Frigate's YOLO, MBL's scene model (Qwen2-VL, about
3.3 GB) and Whisper fit together; a language model on top usually does not. Hailo-Ollama loads its
model when the first question comes, and while it is loaded MBL's scene sentences can fail with
an out-of-memory error in the log (and the other way round). Choose what matters most to you:

- Assistant on the NPU: use Hailo-Ollama and let the companion work without scene sentences (or
  remove Qwen2-VL on the Hardware page).
- Scene sentences: keep Qwen2-VL and run the assistant elsewhere; press **Stop** for Hailo-Ollama.

## Upgrading HailoRT

Upgrade everything together, in this order:

1. Stop Frigate and MBL: `sudo systemctl stop mbl`.
2. Install the new driver, HailoRT and (if you use it) `hailo-gen-ai-model-zoo` packages of the
   same version (`sudo apt-mark unhold` first if you held them), then reboot.
3. Update Frigate's image to the same HailoRT version.
4. In MBL, **Remove** the old models and **Download** them again: models are built for one
   HailoRT version. The same goes for Hailo-Ollama's models.
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
| Frigate says "Failed to connect to server" or "unsupported Hailo architecture: None" | The board has no free connections. Turn off **Let MBL use the NPU** for a moment, restart Frigate, and check that no test programs are left running |
| `hailort.service` restarts over and over (`journalctl -u hailort.service`) | It is a HailoRT 4 leftover. With HailoRT 5: `sudo systemctl disable --now hailort.service` |
| Hardware page says "Hailo-Ollama is not installed" | Install `hailo-gen-ai-model-zoo` of your HailoRT version (see above); `hailo-ollama --help` must work |
| **Start** says "MBL may not use the NPU" | Turn on **Let MBL use the NPU** first |
| **Start** says it "did not answer after starting" | Something else holds port 8000 (`ss -ltnp` lists who listens on `:8000`), or the model zoo and HailoRT versions differ. Run `hailo-ollama` by hand as a test to see its message |
| A model download fails | Hailo's server must have the model for your version; check the Pi's internet connection and try again |
| The assistant test says the model "does not support tools" | Small NPU models often cannot; the assistant still answers but cannot search the library |
| Scene sentences fail while the assistant runs on the NPU | The board is out of memory, see *Memory on the board* |
