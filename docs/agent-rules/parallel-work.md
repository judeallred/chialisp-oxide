# Parallel work

- The owner explicitly requests parallel work whenever it improves progress or
  quality. Delegate independent research, implementation, adversarial-test design,
  documentation and review tasks instead of handling every track serially.
- Give each agent a concrete deliverable, file ownership and relevant constraints.
  For a puzzle, SDK work, simulator test design and tutorial research can proceed
  independently once their policy contract is agreed.
- Keep one agent responsible for integration, shared manifests and final release.
  Coordinate edits to shared files; use isolated targets/worktrees when needed.
- Serialize shared Cargo builds, driver extraction and evidence recording. Never
  rebuild a driver while another task executes it. Freeze sources before capture,
  then run the combined gates and verify publication for the exact commit.
- Batch independent read-only tools in parallel as well. Keep dependent actions,
  approvals and mutations sequential.
- If agent capacity is unavailable, report the limitation honestly, continue useful
  work locally and retry when availability changes. Do not claim parallel agents
  are running when spawning or execution failed.
