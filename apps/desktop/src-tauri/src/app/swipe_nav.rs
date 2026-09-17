//! macOS trackpad swipe navigation.
//!
//! WKWebView never delivers the page-swipe gesture (three-finger swipe or the
//! fluid two-finger page gesture) to the web content, so a DOM listener can't
//! see it. This module installs a process-level `NSEvent` local monitor and
//! forwards gestures to the frontend as Tauri events:
//!
//! - `swipe-nav` — a discrete three-finger swipe (`NSEventTypeSwipe`). Only the
//!   end event carries a direction: `deltaX > 0` is back, `< 0` is forward.
//! - `swipe-progress` / `swipe-end` — a fluid swipe tracked from gesture scroll
//!   events with `trackSwipeEventWithOptions:`. Emits a continuous `amount`
//!   (-1..1) so the frontend can follow the finger, then a commit decision.
//!
//! The frontend keeps `SwipeContext` up to date (setting enabled, pointer over
//! the active list, history availability). Gestures are ignored unless the
//! context allows them, so horizontal scrolling elsewhere is never hijacked.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// What the frontend reports so the native monitor knows when a swipe means navigation.
#[derive(Clone, Copy, Default)]
pub struct SwipeContext {
    pub enabled: bool,
    pub pointer_in_list: bool,
    pub can_back: bool,
    pub can_forward: bool,
}

#[derive(Default)]
pub struct SwipeState(Mutex<SwipeContext>);

impl SwipeState {
    pub fn context(&self) -> SwipeContext {
        self.0.lock().map(|current| *current).unwrap_or_default()
    }

    fn store(&self, context: SwipeContext) {
        if let Ok(mut current) = self.0.lock() {
            *current = context;
        }
    }
}

#[derive(Serialize, Clone, Copy)]
struct SwipeProgress {
    amount: f64,
}

#[derive(Serialize, Clone, Copy)]
struct SwipeEnd {
    committed: bool,
    amount: f64,
}

/// Maps a swipe amount to a navigation direction, respecting available history.
/// Positive is back (fingers move right), negative is forward.
fn direction(can_back: bool, can_forward: bool, amount: f64) -> Option<&'static str> {
    if amount > 0.0 && can_back {
        Some("back")
    } else if amount < 0.0 && can_forward {
        Some("forward")
    } else {
        None
    }
}

#[tauri::command]
pub fn set_swipe_context(
    state: tauri::State<'_, SwipeState>,
    enabled: bool,
    pointer_in_list: bool,
    can_back: bool,
    can_forward: bool,
) {
    state.store(SwipeContext {
        enabled,
        pointer_in_list,
        can_back,
        can_forward,
    });
}

#[cfg(not(target_os = "macos"))]
pub fn init<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(target_os = "macos")]
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_app_kit::{
        NSEvent, NSEventMask, NSEventPhase, NSEventSwipeTrackingOptions, NSEventType,
    };
    use objc2_core_foundation::CGFloat;

    let app_handle = app.clone();
    let tracking = Arc::new(AtomicBool::new(false));
    let tracking_flag = tracking.clone();

    let monitor = RcBlock::new(move |event_ptr: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: the monitor hands us a valid NSEvent for the matched mask.
        let event = unsafe { event_ptr.as_ref() };
        let pass = || event_ptr.as_ptr();
        let state = app_handle.state::<SwipeState>();

        match event.r#type() {
            NSEventType::Swipe => {
                let delta = event.deltaX();
                if delta != 0.0 {
                    let context = state.context();
                    if context.enabled && context.pointer_in_list {
                        if let Some(direction) =
                            direction(context.can_back, context.can_forward, delta)
                        {
                            let _ = app_handle.emit("swipe-nav", direction);
                        }
                    }
                }
            }
            NSEventType::ScrollWheel => {
                // While a swipe is being tracked, swallow the remaining gesture events.
                if tracking_flag.load(Ordering::Relaxed) {
                    return std::ptr::null_mut();
                }
                let phase = event.phase();
                if phase != NSEventPhase::Began && phase != NSEventPhase::Changed {
                    return pass();
                }
                // Trackpads only; a horizontal mouse wheel must keep scrolling.
                if !event.hasPreciseScrollingDeltas() {
                    return pass();
                }
                // Horizontal-dominant gesture only.
                if event.scrollingDeltaX().abs() <= event.scrollingDeltaY().abs() {
                    return pass();
                }
                let context = state.context();
                if !context.enabled || !context.pointer_in_list {
                    return pass();
                }
                if !context.can_back && !context.can_forward {
                    return pass();
                }

                let can_back = context.can_back;
                let can_forward = context.can_forward;
                tracking_flag.store(true, Ordering::Relaxed);
                let flag = tracking_flag.clone();
                let app = app_handle.clone();
                let handler = RcBlock::new(
                    move |amount: CGFloat,
                          phase: NSEventPhase,
                          is_complete: Bool,
                          _stop: NonNull<Bool>| {
                        if phase == NSEventPhase::Cancelled {
                            let _ = app.emit(
                                "swipe-end",
                                SwipeEnd {
                                    committed: false,
                                    amount: 0.0,
                                },
                            );
                            flag.store(false, Ordering::Relaxed);
                            return;
                        }
                        if is_complete.as_bool() {
                            let committed =
                                (amount >= 1.0 && can_back) || (amount <= -1.0 && can_forward);
                            let _ = app.emit("swipe-end", SwipeEnd { committed, amount });
                            flag.store(false, Ordering::Relaxed);
                        } else {
                            let _ = app.emit("swipe-progress", SwipeProgress { amount });
                        }
                    },
                );
                event.trackSwipeEventWithOptions_dampenAmountThresholdMin_max_usingHandler(
                    NSEventSwipeTrackingOptions::LockDirection,
                    -1.0,
                    1.0,
                    &handler,
                );
                return std::ptr::null_mut();
            }
            _ => {}
        }

        pass()
    });

    // SAFETY: the block returns either null or the pointer it was given, both
    // valid per the monitor contract. The monitor must live for the app lifetime.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::Swipe | NSEventMask::ScrollWheel,
            &monitor,
        )
    };
    if let Some(_monitor) = monitor {
        std::mem::forget(_monitor);
    } else {
        eprintln!("liteexplorer: swipe-nav: failed to install NSEvent monitor");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_amount_is_back() {
        assert_eq!(direction(true, false, 0.5), Some("back"));
        assert_eq!(direction(true, true, 0.5), Some("back"));
    }

    #[test]
    fn negative_amount_is_forward() {
        assert_eq!(direction(false, true, -0.5), Some("forward"));
        assert_eq!(direction(true, true, -0.5), Some("forward"));
    }

    #[test]
    fn unavailable_history_is_ignored() {
        assert_eq!(direction(false, false, 1.0), None);
        assert_eq!(direction(false, true, 1.0), None);
        assert_eq!(direction(true, false, -1.0), None);
        assert_eq!(direction(true, true, 0.0), None);
    }
}
