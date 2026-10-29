import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useToast } from "../../context/ToastContext";

type WorkerRow = components["schemas"]["WorkerRow"];
type RemoteNodeRow = components["schemas"]["RemoteNodeRow"];

export function Workers() {
  const { showToast } = useToast();

  const [workers, setWorkers] = createSignal<WorkerRow[]>([]);
  const [nodes, setNodes] = createSignal<RemoteNodeRow[]>([]);
  const [loading, setLoading] = createSignal(true);

  // Pairing code dialog state
  const [pairingWorkerModal, setPairingWorkerModal] = createSignal(false);
  const [workerPairingCode, setWorkerPairingCode] = createSignal<string | null>(null);

  const [pairingNodeModal, setPairingNodeModal] = createSignal(false);
  const [nodePairingCode, setNodePairingCode] = createSignal<string | null>(null);

  const loadAll = async () => {
    setLoading(true);
    try {
      const [workersRes, nodesRes] = await Promise.all([
        client.GET("/api/v1/workers"),
        client.GET("/api/v1/nodes"),
      ]);
      if (workersRes.error || nodesRes.error) throw new Error("Load failed");
      if (Array.isArray(workersRes.data)) {
        setWorkers(workersRes.data as WorkerRow[]);
      }
      if (Array.isArray(nodesRes.data)) {
        setNodes(nodesRes.data as RemoteNodeRow[]);
      }
    } catch {
      showToast({
        title: "Error",
        message: "Failed to load distributed workers and nodes",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    loadAll();
    const timer = setInterval(() => {
      if (!document.hidden) void loadAll();
    }, 10_000);
    onCleanup(() => clearInterval(timer));
  });

  const handleGenerateWorkerCode = async () => {
    try {
      const { data, error } = await client.POST("/api/v1/workers/pairing-code");
      if (error) throw new Error("Pairing failed");
      const codeResp = data as { code?: string } | undefined;
      if (codeResp?.code) {
        setWorkerPairingCode(codeResp.code);
        setPairingWorkerModal(true);
      }
    } catch {
      showToast({
        title: "Error",
        message: "Failed to generate worker pairing code",
        type: "error",
      });
    }
  };

  const handleGenerateNodeCode = async () => {
    try {
      const { data, error } = await client.POST("/api/v1/nodes/pairing-code");
      if (error) throw new Error("Pairing failed");
      const codeResp = data as { code?: string } | undefined;
      if (codeResp?.code) {
        setNodePairingCode(codeResp.code);
        setPairingNodeModal(true);
      }
    } catch {
      showToast({
        title: "Error",
        message: "Failed to generate storage node pairing code",
        type: "error",
      });
    }
  };

  const handleWakeWorker = async (id: number) => {
    try {
      const { error } = await client.POST("/api/v1/workers/{id}/wake", {
        params: { path: { id } },
      });
      if (error) throw new Error("Wake failed");
      showToast({
        title: "Magic Packet Sent",
        message: `Wake-on-LAN packet transmitted to worker ${id}`,
        type: "success",
      });
    } catch {
      showToast({
        title: "Wake Failed",
        message: `Could not wake worker ${id}. Check MAC address registration.`,
        type: "error",
      });
    }
  };

  const handleRevokeWorker = async (id: number) => {
    if (!confirm(`Are you sure you want to revoke worker ${id}?`)) return;
    try {
      const { error } = await client.DELETE("/api/v1/workers/{id}", { params: { path: { id } } });
      if (error) throw new Error("Revoke failed");
      showToast({
        title: "Worker Revoked",
        message: `Worker ${id} has been removed`,
        type: "success",
      });
      loadAll();
    } catch {
      showToast({
        title: "Error",
        message: `Failed to revoke worker ${id}`,
        type: "error",
      });
    }
  };

  const handleRevokeNode = async (id: number) => {
    if (!confirm(`Are you sure you want to revoke storage node ${id}?`)) return;
    try {
      const { error } = await client.DELETE("/api/v1/nodes/{id}", { params: { path: { id } } });
      if (error) throw new Error("Revoke failed");
      showToast({
        title: "Node Revoked",
        message: `Storage node ${id} has been removed`,
        type: "success",
      });
      loadAll();
    } catch {
      showToast({
        title: "Error",
        message: `Failed to revoke node ${id}`,
        type: "error",
      });
    }
  };

  const formatBytes = (bytes?: number | null) => {
    if (!bytes) return "Unknown";
    const tb = bytes / 1_000_000_000_000;
    return `${tb.toFixed(1)} TB`;
  };

  const getStatusBadge = (status: string) => {
    switch (status) {
      case "idle":
        return <Badge variant="success">Idle</Badge>;
      case "busy":
        return <Badge variant="accent">Transcoding</Badge>;
      case "paused_gaming":
        return <Badge variant="warning">Gaming Paused</Badge>;
      case "online":
        return <Badge variant="success">Online</Badge>;
      default:
        return <Badge variant="default">{status || "Offline"}</Badge>;
    }
  };

  return (
    <div class="p-6 max-w-7xl mx-auto space-y-8">
      {/* Header */}
      <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 border-b pb-6">
        <div>
          <h1 class="text-3xl font-bold tracking-tight text-white">Distributed Nodes</h1>
          <p class="text-sm text-gray-400 mt-1">
            Offload heavy transcode tasks to GPU workers and federate remote storage nodes.
          </p>
        </div>
        <div class="flex items-center gap-3">
          <Button variant="secondary" onClick={handleGenerateWorkerCode}>
            Pair Transcode Worker
          </Button>
          <Button variant="primary" onClick={handleGenerateNodeCode}>
            Pair Storage Node
          </Button>
        </div>
      </div>

      {/* Workers Section */}
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <h2 class="text-xl font-semibold text-white">GPU Transcode Workers</h2>
          <span class="text-xs text-gray-400">{workers().length} registered</span>
        </div>

        <Show
          when={!loading() && workers().length > 0}
          fallback={
            <div class="rounded-xl border border-dashed border-gray-800 p-8 text-center text-gray-400">
              {loading()
                ? "Loading workers..."
                : "No transcode workers connected yet. Click 'Pair Transcode Worker' above."}
            </div>
          }
        >
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <For each={workers()}>
              {(w) => (
                <div class="rounded-xl bg-gray-900 border border-gray-800 p-5 space-y-4">
                  <div class="flex items-start justify-between">
                    <div>
                      <h3 class="font-bold text-white text-lg">{w.name}</h3>
                      <p class="text-xs text-gray-400">{w.hostname || "Unknown host"}</p>
                    </div>
                    {getStatusBadge(w.status)}
                  </div>

                  <div class="space-y-1 text-xs text-gray-300">
                    <p>
                      <span class="text-gray-500">GPU:</span> {w.capabilities.gpuDeviceName} (
                      {w.capabilities.gpuVendor})
                    </p>
                    <p>
                      <span class="text-gray-500">Max Sessions:</span> {w.maxSessions}
                    </p>
                    <p>
                      <span class="text-gray-500">MAC:</span> {w.macAddress || "Not registered"}
                    </p>
                    <p>
                      <span class="text-gray-500">Encoders:</span>{" "}
                      {w.capabilities.supportedVideoEncoders.join(", ") || "None"}
                    </p>
                  </div>

                  <div class="pt-2 border-t border-gray-800 flex items-center justify-between">
                    <Button
                      variant="secondary"
                      onClick={() => handleWakeWorker(w.id)}
                      disabled={!w.macAddress}
                    >
                      Wake (WoL)
                    </Button>
                    <Button variant="danger" onClick={() => handleRevokeWorker(w.id)}>
                      Revoke
                    </Button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* Storage Nodes Section */}
      <div class="space-y-4 pt-6 border-t border-gray-800">
        <div class="flex items-center justify-between">
          <h2 class="text-xl font-semibold text-white">Federated Storage Nodes</h2>
          <span class="text-xs text-gray-400">{nodes().length} registered</span>
        </div>

        <Show
          when={!loading() && nodes().length > 0}
          fallback={
            <div class="rounded-xl border border-dashed border-gray-800 p-8 text-center text-gray-400">
              {loading()
                ? "Loading storage nodes..."
                : "No federated storage nodes registered yet."}
            </div>
          }
        >
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <For each={nodes()}>
              {(n) => (
                <div class="rounded-xl bg-gray-900 border border-gray-800 p-5 space-y-4">
                  <div class="flex items-start justify-between">
                    <div>
                      <h3 class="font-bold text-white text-lg">{n.name}</h3>
                      <p class="text-xs text-blue-400">{n.baseUrl}</p>
                    </div>
                    {getStatusBadge(n.status)}
                  </div>

                  <div class="space-y-1 text-xs text-gray-300">
                    <p>
                      <span class="text-gray-500">Storage:</span>{" "}
                      {formatBytes(n.capabilities.storageAvailableBytes)} free of{" "}
                      {formatBytes(n.capabilities.storageTotalBytes)}
                    </p>
                    <p>
                      <span class="text-gray-500">HW Transcode:</span>{" "}
                      {n.capabilities.supportsHardwareTranscode ? "Supported" : "No"}
                    </p>
                  </div>

                  <div class="pt-2 border-t border-gray-800 flex items-center justify-end">
                    <Button variant="danger" onClick={() => handleRevokeNode(n.id)}>
                      Revoke
                    </Button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* Worker Pairing Dialog */}
      <Show when={pairingWorkerModal()}>
        <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
          <div class="bg-gray-900 border border-gray-800 rounded-2xl p-6 max-w-md w-full space-y-5">
            <h3 class="text-xl font-bold text-white">Pair Transcode Worker</h3>
            <p class="text-sm text-gray-400">
              Use this one-time pairing code on your GPU machine. The code expires in 15 minutes.
            </p>

            <div class="p-4 bg-gray-950 rounded-xl text-center border border-gray-800">
              <span class="text-3xl font-mono tracking-widest font-extrabold text-blue-400">
                {workerPairingCode()}
              </span>
            </div>

            <div class="text-xs font-mono bg-gray-950 p-3 rounded-lg text-gray-300 break-all border border-gray-800">
              <code>
                mbl-worker pair --server {window.location.origin} --code {workerPairingCode()}
              </code>
            </div>

            <div class="flex justify-end pt-2">
              <Button
                variant="primary"
                onClick={() => {
                  setPairingWorkerModal(false);
                  loadAll();
                }}
              >
                Done
              </Button>
            </div>
          </div>
        </div>
      </Show>

      {/* Node Pairing Dialog */}
      <Show when={pairingNodeModal()}>
        <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
          <div class="bg-gray-900 border border-gray-800 rounded-2xl p-6 max-w-md w-full space-y-5">
            <h3 class="text-xl font-bold text-white">Pair Storage Node</h3>
            <p class="text-sm text-gray-400">
              Use this one-time pairing code to pair a NAS or secondary library server.
            </p>

            <div class="p-4 bg-gray-950 rounded-xl text-center border border-gray-800">
              <span class="text-3xl font-mono tracking-widest font-extrabold text-green-400">
                {nodePairingCode()}
              </span>
            </div>

            <div class="text-xs font-mono bg-gray-950 p-3 rounded-lg text-gray-300 break-all border border-gray-800">
              <code>
                mbl-node pair --server {window.location.origin} --code {nodePairingCode()}{" "}
                --base-url &lt;NODE_URL&gt;
              </code>
            </div>

            <div class="flex justify-end pt-2">
              <Button
                variant="primary"
                onClick={() => {
                  setPairingNodeModal(false);
                  loadAll();
                }}
              >
                Done
              </Button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default Workers;
