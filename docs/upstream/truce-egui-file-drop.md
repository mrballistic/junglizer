# Draft issue: truce-egui ignores file drag and drop (not filed)

Draft for the truce-audio/truce tracker. Not filed; filing is Todd's call.

## Title

truce-egui: file drops are rejected on macOS and Windows because baseview drag events fall into the `_ => Ignored` arm

## Symptom

An egui editor built on truce-egui 6.3.0 never receives dropped files. Dragging a file from Finder or Explorer onto the plug-in window is refused (the cursor shows that a drop is not allowed), and nothing reaches egui: `ctx.input(|i| i.raw.hovered_files)` and `i.raw.dropped_files` are always empty. This applies on macOS and Windows; the cause below is the same for both.

## Cause

baseview-truce 0.1.1-truce.13 already does the platform work. On macOS it registers the view for file drags (`registerForDraggedTypes`), and on Windows it calls `RegisterDragDrop`. It then delivers `MouseEvent::DragEntered`, `DragMoved`, `DragLeft` and `DragDropped` to the window handler, with the paths in `DropData::Files`.

`EguiWindowHandler::on_event` in `crates/truce-egui/src/editor.rs` has no arms for those variants, so they reach the wildcard `_ => EventStatus::Ignored`. baseview turns `Ignored` into `NSDragOperationNone` on macOS and `DROPEFFECT_NONE` on Windows, so the OS rejects the drag before it can be dropped. The paths are also never copied into `egui::RawInput`, so egui has no way to see them even if the drag were accepted.

Linux is unaffected by this change: baseview has no drop support on X11, so those events never arrive there.

## Proposed fix

1. Keep `hovered_files` and `dropped_files` on the handler.
2. On `DragEntered` and `DragMoved`: move the pointer (as `CursorMoved` does, so egui knows where the drag is), record the hovered paths, and return `EventStatus::AcceptDrop(DropEffect::Copy)` when the data is `DropData::Files`. Anything else stays `Ignored`.
3. On `DragLeft`: clear the hovered files and push `PointerGone`, which also triggers a repaint so the UI can clear any hover state.
4. On `DragDropped`: move the pointer and clear the hovered files. If the data is `DropData::Files`, move the paths into `dropped_files` and accept the drop; otherwise return `Ignored`.
5. Pass both lists through `RawInput` each frame (`hovered_files` cloned, `dropped_files` taken), which is what egui-winit does.

With the drag arms added the match is exhaustive, so the wildcard becomes an unreachable pattern and is removed.

We carry this patch in a vendored copy of truce-egui 6.3.0. It builds with no new warnings and is rustfmt clean. Happy to open a PR if this approach suits you.

## Patch

Against v6.3.0 (`ff6b573c`):

```diff
--- a/crates/truce-egui/src/editor.rs
+++ b/crates/truce-egui/src/editor.rs
@@ -385,6 +385,12 @@
     #[cfg(target_os = "windows")]
     render_thread: Option<RenderThread>,
     pending_events: Vec<egui::Event>,
+    /// Files currently dragged over the window (baseview `DragEntered` /
+    /// `DragMoved`), mirrored into every frame's `RawInput`.
+    hovered_files: Vec<egui::HoveredFile>,
+    /// Files dropped since the last frame (baseview `DragDropped`), handed
+    /// to egui once and then cleared.
+    dropped_files: Vec<egui::DroppedFile>,
     modifiers: egui::Modifiers,
     start_time: std::time::Instant,
     size: (u32, u32),
@@ -709,6 +715,8 @@
             time: Some(self.start_time.elapsed().as_secs_f64()),
             modifiers: self.modifiers,
             events: std::mem::take(&mut self.pending_events),
+            hovered_files: self.hovered_files.clone(),
+            dropped_files: std::mem::take(&mut self.dropped_files),
             focused: true,
             ..Default::default()
         };
@@ -994,12 +1002,11 @@
                 Event::Mouse(mouse) => {
                     use baseview::MouseEvent::{
                         ButtonPressed, ButtonReleased, CursorEntered, CursorLeft, CursorMoved,
-                        WheelScrolled,
+                        DragDropped, DragEntered, DragLeft, DragMoved, WheelScrolled,
                     };
                     // The explicit `CursorEntered => Ignored` arm signals the
                     // event was considered and intentionally ignored (vs.
-                    // `CursorLeft` which we forward as `PointerGone`); the
-                    // wildcard absorbs future baseview MouseEvent variants.
+                    // `CursorLeft` which we forward as `PointerGone`).
                     #[allow(clippy::match_same_arms)]
                     match mouse {
                         CursorMoved {
@@ -1072,7 +1079,70 @@
                             self.pending_events.push(egui::Event::PointerGone);
                             EventStatus::Captured
                         }
-                        _ => EventStatus::Ignored,
+                        // Returning `Ignored` here would reject the drag on
+                        // macOS (NSDragOperationNone) and Windows
+                        // (DROPEFFECT_NONE), so accept file drags as a copy
+                        // and hand the paths to egui. These arms make the match
+                        // exhaustive, so the old `_ => Ignored` wildcard is
+                        // removed (it would be an unreachable pattern).
+                        DragEntered {
+                            position,
+                            modifiers,
+                            data,
+                        }
+                        | DragMoved {
+                            position,
+                            modifiers,
+                            data,
+                        } => {
+                            self.modifiers = convert_kb_modifiers(modifiers);
+                            #[allow(clippy::cast_possible_truncation)]
+                            let pos = egui::pos2(position.x as f32, position.y as f32);
+                            self.last_cursor_pos = pos;
+                            self.pending_events.push(egui::Event::PointerMoved(pos));
+                            if let baseview::DropData::Files(paths) = data {
+                                self.hovered_files = paths
+                                    .into_iter()
+                                    .map(|p| egui::HoveredFile {
+                                        path: Some(p),
+                                        ..Default::default()
+                                    })
+                                    .collect();
+                                EventStatus::AcceptDrop(baseview::DropEffect::Copy)
+                            } else {
+                                self.hovered_files.clear();
+                                EventStatus::Ignored
+                            }
+                        }
+                        DragLeft => {
+                            self.hovered_files.clear();
+                            // Also queues a repaint so the hover state clears.
+                            self.pending_events.push(egui::Event::PointerGone);
+                            EventStatus::Captured
+                        }
+                        DragDropped {
+                            position,
+                            modifiers,
+                            data,
+                        } => {
+                            self.modifiers = convert_kb_modifiers(modifiers);
+                            #[allow(clippy::cast_possible_truncation)]
+                            let pos = egui::pos2(position.x as f32, position.y as f32);
+                            self.last_cursor_pos = pos;
+                            self.pending_events.push(egui::Event::PointerMoved(pos));
+                            self.hovered_files.clear();
+                            if let baseview::DropData::Files(paths) = data {
+                                self.dropped_files.extend(paths.into_iter().map(|p| {
+                                    egui::DroppedFile {
+                                        path: Some(p),
+                                        ..Default::default()
+                                    }
+                                }));
+                                EventStatus::AcceptDrop(baseview::DropEffect::Copy)
+                            } else {
+                                EventStatus::Ignored
+                            }
+                        }
                     }
                 }
                 Event::Keyboard(kb) => {
@@ -1510,6 +1580,8 @@
                     #[cfg(target_os = "windows")]
                     render_thread,
                     pending_events: Vec::new(),
+                    hovered_files: Vec::new(),
+                    dropped_files: Vec::new(),
                     modifiers: egui::Modifiers::NONE,
                     start_time: std::time::Instant::now(),
                     size,
```
