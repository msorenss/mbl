/** Paths that can never be hidden, so the menu always leads somewhere. */
export const ALWAYS_VISIBLE = new Set(["/"]);

/**
 * Entries in the user's order (unknown or new entries keep their default place after the
 * ordered ones), without the hidden ones unless `includeHidden`.
 */
export function arrangeNav<T extends { href: string }>(
  links: readonly T[],
  order: readonly string[],
  hidden: readonly string[],
  includeHidden = false,
): T[] {
  const rank = new Map(order.map((href, i) => [href, i]));
  return links
    .map((link, i) => ({ link, i }))
    .filter(
      ({ link }) => includeHidden || ALWAYS_VISIBLE.has(link.href) || !hidden.includes(link.href),
    )
    .sort((a, b) => {
      const ra = rank.get(a.link.href) ?? Number.MAX_SAFE_INTEGER;
      const rb = rank.get(b.link.href) ?? Number.MAX_SAFE_INTEGER;
      return ra !== rb ? ra - rb : a.i - b.i;
    })
    .map(({ link }) => link);
}

/** Moves `href` one step up (-1) or down (+1) within `group`, returning the new full order. */
export function moveInOrder(
  group: readonly string[],
  fullOrder: readonly string[],
  href: string,
  step: -1 | 1,
): string[] {
  const list = [...group];
  const at = list.indexOf(href);
  const to = at + step;
  if (at < 0 || to < 0 || to >= list.length) return [...fullOrder];
  [list[at], list[to]] = [list[to], list[at]];
  // Entries of this group take their new order; others keep theirs.
  const others = fullOrder.filter((h) => !group.includes(h));
  return [...others, ...list];
}
