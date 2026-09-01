# Telorgon Repository Guidance

## Execution safety

Do not run GUI applications, examples, servers, services, or background processes. Compilation,
formatting, linting, unit tests, compile-only GPU tests, and documentation validation are allowed.
Leave interactive and hardware-presenting application runs to the user.

## Commit-message handoff

After any task that changes repository files, include a short, one-line suggested commit message in
the final response. Summarize the completed change in imperative mood. Generate the message only; do
not create a Git commit unless the user explicitly asks.

## Required architecture reading

Before changing architecture or graphics code, read the relevant documents under `docs/`, starting
with `docs/README.md`. Treat target documents as proposals and `docs/IMPLEMENTATION_STATUS.md` as the
authority for what currently works.

## Adjacent reference-source requirement

The repository has a read-only source library at `../other-rendering-libs`. Before designing or
implementing graphics backends, presentation, resource lifetime, synchronization, external images,
render planning, UI batching, shell composition, or platform embedding:

1. read `docs/REFERENCE_IMPLEMENTATIONS.md`;
2. inspect the relevant source paths named in its routing matrix;
3. compare at least two independent implementations for a cross-backend contract or a bug-prone GPU
   mechanism;
4. verify behavior against the official graphics API specification or vendor documentation;
5. record the inspected paths, extracted invariants, rejected alternatives, and tests derived from
   the review in the task handoff or architecture note; and
6. implement Telorgon's requirements rather than copying another project's public abstraction.

The adjacent projects are reference material only. Do not modify them, run their applications, add
them as dependencies, vendor them into Telorgon, or copy code without explicit license and provenance
review. Preserve their repository-specific instructions if work is ever explicitly authorized inside
one of those repositories.
