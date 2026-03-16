# oldcrap

This directory is intentional.

`oldcrap/` is the quarantine zone for code and assets that were once live, but
were swept out of the active tree during the `usit-qt` reconstruction. The name
is irreverent on purpose: it is meant to discourage casual reuse and make it
obvious that nothing in here should be treated as current architecture by
default.

What belongs here:

- retired entrypoints and orchestration code
- old UI shells and rendering paths
- tests tied to the retired architecture
- historical manifests and branch-specific notes that are still useful as
  references during reintroduction

What does not follow automatically from being here:

- the code is bad
- the code is doomed
- the code should never come back

Much of this material may be reintroduced piecemeal. The point of the quarantine
is to force deliberate re-entry instead of letting legacy structure silently
govern the rebuild.

Rule of thumb:

- copy or port specific ideas back out
- do not wholesale "un-oldcrap" a subtree unless we have consciously decided the
  architecture deserves it
