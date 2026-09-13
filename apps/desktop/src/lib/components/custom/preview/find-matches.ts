export function findMatches(
  text: string,
  query: string,
  limit = Number.POSITIVE_INFINITY,
): Array<[number, number]> {
  if (!query) return [];
  const haystack = text.toLowerCase();
  const needle = query.toLowerCase();
  const matches: Array<[number, number]> = [];
  let position = haystack.indexOf(needle);
  while (position !== -1 && matches.length < limit) {
    matches.push([position, position + needle.length]);
    position = haystack.indexOf(needle, position + needle.length);
  }
  return matches;
}
