# Contributing to Junglizer

Thanks for your interest. Junglizer is a small project with one maintainer, so contributions work a little differently here than on a big community project. Please read this before you open a pull request.

## The short version

- **Bug reports and ideas are welcome.** Open an issue.
- **Pull requests are by invitation.** Only collaborators can open pull requests in this repository. Open an issue first; if the maintainer agrees to the change, they’ll invite you as a collaborator for it.
- **Small is good.** One focused change per pull request.

## Reporting bugs

Open an issue with:

- your OS and version, the host (DAW) and its version, and the plugin format (AUv2, VST3 or CLAP)
- the Junglizer version, from the release you downloaded
- what you did, what you expected, and what happened instead
- if a break file is involved, its format, length, sample rate and filename (the filename matters: Junglizer reads the tempo from it)

Please don’t attach commercial sample-pack audio to an issue. Describe the file instead, or share a short loop you made yourself.

## Suggesting features

Open an issue describing the problem you want solved, not just the feature. Junglizer is deliberately focused (a four-bar break instrument), so some good ideas won’t fit. A “no” isn’t a judgment of the idea.

## Security issues

Don’t open a public issue. Use GitHub’s **Report a vulnerability** button on the Security tab, which reaches the maintainer privately. Project files and loaded audio are untrusted input, so path handling and audio decoding are the areas most worth a look.

## Pull requests

**Before you write code:**

1. Find or open an issue for the change.
2. Wait for the maintainer to agree on the approach. Issues ready for outside help are labeled `help wanted`.
3. The maintainer invites you as a collaborator so you can open the pull request. Collaborator access is for that work and may be removed afterwards.

For an obvious typo or broken link in the docs, just open an issue that points at it.

**What a pull request needs:**

- **A linked, agreed issue** (“Closes #123”).
- **One change.** No drive-by refactors, reformatting or dependency bumps alongside it.
- **Passing checks:** `cargo fmt --all -- --check`, `cargo test --locked` and `cargo test --locked --features rt-paranoid`. CI runs `test` on every pull request, and `main` won’t accept a merge without it.
- **Tests** for behavior changes. The audio thread must never allocate, free, block or do file I/O; the rt-paranoid tests enforce this, so keep them green.
- **No new dependencies** without agreeing on them in the issue first. Any dependency must be license-compatible with GPL-3.0-or-later (MIT, Apache-2.0, BSD, Zlib, MPL-2.0 and similar are fine).
- **UI copy** follows the project’s style: smart punctuation (’ “ ”), US English, and em dashes only where they earn their place.
- **Your own understanding.** You must be able to explain every line you submit. Pull requests that look generated in bulk, or that the author can’t discuss, will be closed.

See [docs/DEVELOPING.md](docs/DEVELOPING.md) for building, the architecture, tests and tools.

## Audio and other assets

Don’t contribute audio, images or other assets unless you made them yourself or they carry a license that allows redistribution under the project’s terms. Never submit audio from commercial sample packs, even if you own the pack: most pack licenses forbid redistribution.

## Licensing of contributions

Junglizer is licensed under the [GNU General Public License v3.0 or later](LICENSE). By submitting a contribution, you agree that it is licensed under the same terms (“inbound = outbound”).

Sign off every commit to certify that you wrote it or otherwise have the right to submit it under that license, per the [Developer Certificate of Origin](https://developercertificate.org/):

```sh
git commit -s
```

This adds a `Signed-off-by: Your Name <you@example.com>` line. Pull requests with unsigned commits can’t be merged.

## What to expect

This is a spare-time project. Reviews happen when the maintainer has time, and there is no response-time promise. The maintainer may decline, close or rework any contribution, and has the final say on what ships.

## Conduct

Be kind and constructive. Harassment, personal attacks and spam get you blocked from the repository without further discussion.
