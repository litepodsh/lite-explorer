import { invoke } from "@tauri-apps/api/core";

export type CalendarEvent = {
  kind: string;
  summary: string;
  start: string | null;
  end: string | null;
  allDay: boolean;
  location: string;
  description: string;
  organizer: string;
  uid: string;
};

export type CalendarPreview = {
  name: string;
  events: CalendarEvent[];
};

/** Parses events, todos and journal entries from an `.ics` calendar. */
export function openCalendar(path: string): Promise<CalendarPreview> {
  return invoke<CalendarPreview>("open_calendar", { path });
}
