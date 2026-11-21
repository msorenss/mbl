import { expect, it } from "vitest";
import { arrangeNav, moveInOrder } from "./navLayout";

const links = [{ href: "/" }, { href: "/libraries" }, { href: "/search" }, { href: "/livetv" }];

it("orders and hides menu entries, keeping new ones in their default place", () => {
  expect(arrangeNav(links, [], []).map((l) => l.href)).toEqual([
    "/",
    "/libraries",
    "/search",
    "/livetv",
  ]);
  expect(arrangeNav(links, ["/livetv", "/search"], ["/libraries"]).map((l) => l.href)).toEqual([
    "/livetv",
    "/search",
    "/",
  ]);
  // Home cannot be hidden; hidden ones are listed when editing.
  expect(arrangeNav(links, [], ["/"]).map((l) => l.href)).toContain("/");
  expect(arrangeNav(links, [], ["/search"], true)).toHaveLength(4);
});

it("moves an entry within its group", () => {
  const group = ["/", "/libraries", "/search"];
  expect(moveInOrder(group, [], "/search", -1)).toEqual(["/", "/search", "/libraries"]);
  expect(moveInOrder(group, ["/admin/logs"], "/", 1)).toEqual([
    "/admin/logs",
    "/libraries",
    "/",
    "/search",
  ]);
  // At the edge nothing changes.
  expect(moveInOrder(group, ["/x"], "/", -1)).toEqual(["/x"]);
});
