# Release Notes

Material changes receive one task-named note under `unreleased/`. Notes describe
observable impact and operational action without turning planned behavior into
a current compatibility claim.

Copy `TEMPLATE.md`, replace every placeholder, and name the file
`<TASK-ID>.md`. One task has at most one unreleased note; related pull requests
update that note. M10-001 will define release versions, changelog assembly, and
the process for moving notes out of `unreleased/`.

Allowed change types are `Added`, `Changed`, `Fixed`, `Security`, `Deprecated`,
and `Removed`. Internal work may omit a note only when the pull request gives a
reviewed `N/A` justification.
