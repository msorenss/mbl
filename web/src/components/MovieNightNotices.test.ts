import { expect, it } from "vitest";
import { movieNightNotice } from "./MovieNightNotices";

it("describes invitations and results, but not your own invitation", () => {
  const invited = {
    type: "movieNightScheduled",
    data: { sessionId: "mm_1", startsAt: Date.UTC(2026, 10, 27, 19), hostId: 1, host: "Anna" },
  };
  expect(movieNightNotice(invited, 2)).toMatch(
    /^Anna invited you to movie night .+\. Open Movie night to vote\.$/,
  );
  expect(movieNightNotice(invited, 1)).toBeNull();
  expect(
    movieNightNotice({ type: "movieNightDecided", data: { sessionId: "mm_1", title: "Up" } }, 2),
  ).toBe("Movie night: tonight's pick is Up.");
  expect(
    movieNightNotice({ type: "movieNightDecided", data: { sessionId: "mm_1", title: null } }, 2),
  ).toBe("Movie night: voting closed and no title got a like.");
  expect(movieNightNotice({ type: "jobUpdated", data: {} }, 2)).toBeNull();
});
