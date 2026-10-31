import { expect, it } from "vitest";
import { castDiagnostics } from "./castDiagnostics";

it("masks URL credentials, queries, fragments and bearer tokens without traversing SDK objects", () => {
  castDiagnostics.start();
  const circular: Record<string, unknown> = {};
  circular.self = circular;
  castDiagnostics.record("test", {
    url: "http://admin:secret@tv.test/video.m3u8?token=signed-secret#fragment-secret",
    authorization: "hidden-secret",
    sdk: circular,
    value: Number.NaN,
  });
  castDiagnostics.error("failure", new Error("Bearer bearer-secret token=raw-secret"));
  const report = castDiagnostics.export();
  for (const secret of [
    "secret@",
    "signed-secret",
    "fragment-secret",
    "hidden-secret",
    "bearer-secret",
    "raw-secret",
  ])
    expect(report).not.toContain(secret);
  expect(report).toContain("http://tv.test/video.m3u8?[redacted]");
  expect(JSON.parse(report).events[1].details.value).toBeNull();
});

it("retains the first negotiation and recent events while bounding the log", () => {
  castDiagnostics.start();
  castDiagnostics.record("load.send");
  for (let i = 0; i < 300; i++) castDiagnostics.record("receiver.status", { position: i });
  const events = JSON.parse(castDiagnostics.export()).events;
  expect(events).toHaveLength(200);
  expect(events[1].stage).toBe("load.send");
  expect(events.at(-1).details.position).toBe(299);
  castDiagnostics.start();
  expect(JSON.parse(castDiagnostics.export()).events).toHaveLength(1);
});

it("captures Cast error fields and stops notifying disposed subscribers", () => {
  castDiagnostics.start();
  let updates = 0;
  const off = castDiagnostics.subscribe(() => updates++);
  castDiagnostics.error("load.failed", {
    code: "LOAD_MEDIA_FAILED",
    description: "bad stream",
    detailedErrorCode: 104,
  });
  off();
  castDiagnostics.record("later");
  expect(updates).toBe(1);
  expect(JSON.parse(castDiagnostics.export()).events[1].details).toEqual({
    code: "LOAD_MEDIA_FAILED",
    description: "bad stream",
    detailedErrorCode: 104,
  });
});

it("masks capability IDs in relative and absolute HLS and live paths", () => {
  castDiagnostics.start();
  castDiagnostics.record("load.send", {
    hls: "http://tv.test/api/v1/stream/hls/capability-secret/master.m3u8?signature=query-secret",
    live: "/api/v1/stream/live/live-secret/0.m4s?other=other-secret",
  });
  const report = castDiagnostics.export();
  for (const secret of ["capability-secret", "live-secret", "query-secret", "other-secret"])
    expect(report).not.toContain(secret);
  expect(report).toContain("/stream/hls/[session]/master.m3u8?[redacted]");
});
