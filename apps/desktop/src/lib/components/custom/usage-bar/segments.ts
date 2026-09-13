export type UsageSegment = {
  id: string;
  value: number;
  /** Any CSS color. Defaults to the app accent. */
  color?: string;
  label?: string;
};

export type SizedSegment = UsageSegment & { percent: number };

/** Turns raw values into percentages of `total`, never adding up past 100. */
export function sizeSegments(segments: UsageSegment[], total: number): SizedSegment[] {
  let left = 100;
  return segments.map((segment) => {
    const raw = total > 0 ? (Math.max(0, segment.value) / total) * 100 : 0;
    const percent = Math.min(raw, left);
    left -= percent;
    return { ...segment, percent };
  });
}
