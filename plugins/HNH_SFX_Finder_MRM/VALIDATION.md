# v0.10.2 validation

Thêm kiểm thử nguồn ngắn hơn I/O, nguồn dài bị giới hạn tại Out, đoạn waveform In/Out, vùng kiểm tra track theo độ dài thực tế và thứ tự ưu tiên SFX vừa vùng trên giao diện.

114 Node tests: existing library/backup/Auto Bin/AV/insertion tests plus smart audio selection, full interval overlap, lock/disabled tracks, top/bottom creation, ignored native insertion option, changed playhead, source trim, rollback and drag sender/file/token validation.

Browser tests with Edge and mocked bridge: audio options persistence, playhead/I/O request wiring, drag gesture IPC, Auto Bin defaults, responsive popup, existing Auto Bin undo/recovery/configuration, fixed video/image insertion footer and session state preservation.

Local Resolve SDK documents AddTrack('audio', {audioType:'stereo', index:1}) and indexed track access. Native dragging follows https://www.electronjs.org/docs/latest/tutorial/native-file-drag-drop . WorkflowIntegration.node and original Resolve import/legacy-insert section are unchanged (package verification).

Limitations: no live Resolve insertion or OS-level drag/drop test performed for this release. Bridge calls are mocked in tests. Verify A1 creation on a scratch timeline with existing audio, trimmed insertion and I/O insertion; verify native drag into Resolve on the user's installed version before public release. Files with unsupported audio mapping fail with a message. A created empty track is retained if a later insertion fails; only returned newly inserted clips may be rolled back.
