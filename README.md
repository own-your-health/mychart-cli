# mychart

Pull your own health records, and your children's through MyChart proxy
access, from any health system that runs Epic, through Epic's
patient-access SMART on FHIR API.

Unofficial; not affiliated with or endorsed by Epic Systems Corporation.
MyChart is Epic's trademark.

```sh
mychart orgs search "<health system>"              # Epic's directory, cached weekly
mychart profiles add adult1-org                    # one per MyChart account at an organization
export MYCHART_PROFILE=adult1-org                  # or --profile adult1-org on each command
mychart auth login --org <ID> --label adult1       # sign in in the profile's Chrome window
mychart patients list
medrec --profile <FOLDER> import --from mychart --from-profile adult1-org --org <ID> --patient adult1   # runs the export
mychart records export --patient adult1 --ndjson    # the records-stream/v1 stream medrec reads
mychart documents list --patient adult1
mychart documents download <ID> -o note.html --patient adult1
mychart api GET Patient/<id> --patient adult1
```

`mychart` only reads. Records go where you send them; nothing is sent
anywhere else.

## What this app does with your records

`mychart` runs on your own computer. There is no mychart server, account
or company between you and your health system: each household registers
its own copy of the app with Epic.

- **What it reads.** The records your health system lets your MyChart
  account share with apps, over Epic's patient-access FHIR API: allergies,
  conditions, medications, immunizations, lab results and vital signs,
  procedures, encounters, care plans and goals, clinical notes, and the
  reports and scanned documents attached to them. For a child, only what
  your proxy access in MyChart allows. MyChart may let you choose what to
  share when you sign in.
- **What it changes.** Nothing. It never writes to your health system.
- **Where records go.** To the computer it runs on: printed to its output
  or saved to a file or folder you name. It sends them to no other server,
  sells and shares nothing, and has no analytics or tracking.
- **What it keeps.** Your sign-ins (access tokens and a key made on your
  computer), encrypted under `~/.config/mychart/vault/` with one key in the
  macOS Keychain; your profiles and who each one signed in (patient ids and
  labels, no record contents) in `~/.config/mychart/config.json`; and each
  profile's Chrome profile, which keeps your health system's sign-in
  cookies.
- **Ending access.** `mychart auth logout` deletes the stored sign-in.
  MyChart's "Manage My Linked Apps and Devices" ends the app's access at
  your health system. Access also ends on its own after the period you
  chose in MyChart.

The source code is public in this repository, and anyone can read what it
does.

## Before the first sign-in

Each household registers its own free patient app with Epic and gives
`mychart` the two client ids: [docs/registration.md](docs/registration.md).

```sh
mychart config set sandbox_client_id <Non-Production Client ID>
mychart config set client_id <Client ID>
mychart config set redirect_uri https://piekstra.github.io/mychart-cli/callback
```

## Install

```sh
curl -sSL https://github.com/piekstra/mychart-cli/releases/latest/download/mychart-aarch64-apple-darwin.tar.gz | tar xz
mv mychart ~/.local/bin/        # or anywhere on PATH
mychart self-update             # later releases
```

Release assets: `mychart-<target>.tar.gz` and `.sha256` for
`aarch64-apple-darwin`, `x86_64-apple-darwin` and
`x86_64-unknown-linux-gnu`. From source: `make install`.

## Commands

| Command | What it does |
|---|---|
| `orgs refresh\|list\|search` | Epic's directory of organizations (`org-list/v1`), downloaded at most weekly into `~/.cache/mychart` |
| `profiles list\|add\|remove\|migrate` | The logins this install signs in as, one per MyChart account at an organization (`profile-list/v1`); `migrate [--org <ORG>] [--patient <WHO>]` moves one login's sign-ins from mychart 0.4 into one, for good |
| `auth login --org <ORG> [--label <WHO>]` | Standalone patient launch in the profile's Chrome window (PKCE), then Epic's dynamic client registration so access lasts for the period chosen in MyChart |
| `auth status` / `auth logout [--forget]` | Each of the profile's sign-ins and its state (`auth-status/v1`); remove one or all, and with `--forget` the settings other than profiles |
| `auth probe --org <ORG>` | Whether the organization knows this app's client id and redirect, without signing in (`auth-probe/v1`) |
| `patients list [--status]` | The patients signed in, each with `self` or `proxy` (`patient-list/v1`) |
| `records export --patient <ID> [--since D] --ndjson` | Every resource the organization serves, documents included, as `records-stream/v1` |
| `records export --patient <ID> -o <DIR>` | The same stream into `<DIR>/records.ndjson`, for Epic's sandbox and test servers only (`records-export/v1`); one cut short goes to `records.incomplete.ndjson` |
| `documents list\|download` | Notes, reports and scans (`documents/v1`) |
| `api GET <path>` | One FHIR read as the signed-in patient |
| `config path\|show\|set\|unset` | Client ids, redirect port and https redirect page, scope, directory URL; `show` lists profiles by name and patient count only |
| `info`, `completions`, `self-update` | Family standard |

Every command takes `--json`. Every command that uses a sign-in (`auth
login|status|logout`, `patients`, `records`, `documents`, `api`) takes
`--profile <NAME>`, or `MYCHART_PROFILE`; there is no default, so one
login's command never runs as another's. Exit codes: 0 ok, 2 usage, 3
sign-in needed, 4 not found, 5 the organization or network failed, 6
confirmation needed (`profiles remove` without `--force`).

## How sign-in works

1. `mychart` reads the organization's endpoints from its FHIR server and
   opens its MyChart sign-in page in a Chrome window of the profile's own,
   so one login's cookies never sign in another. A parent with proxy
   access picks the child there; each person is one sign-in. Without
   Chrome it falls back to the default browser, whose cookies every
   profile shares.
2. MyChart redirects to `http://127.0.0.1:7468/callback`, where `mychart`
   listens. Health systems in production take only an https redirect, so
   with `redirect_uri` set MyChart redirects to
   `https://piekstra.github.io/mychart-cli/callback` instead, a page in
   this repository that passes the answer on to the same address. Its
   [Terms of Use](https://piekstra.github.io/mychart-cli/terms) and
   [privacy](https://piekstra.github.io/mychart-cli/privacy) pages sit
   beside it.
3. `mychart` exchanges the code (PKCE), registers a key made on this
   computer with the organization (Epic's dynamic client registration), and
   from then on gets access tokens by signing with that key, until the
   access period chosen in MyChart ends. MyChart asks how long to share;
   `auth login` says to choose the longest (1 year on Epic's sandbox).

Tokens and the key are one encrypted file per profile and patient under
`~/.config/mychart/vault/` (`<profile>/<organization>/<patient>`), sealed
with one key in the macOS Keychain (`piekstra.mychart`, account
`vault-key`), so a newly built mychart asks macOS once, however many
sign-ins it keeps. Nothing secret is written anywhere else. The vault and
each profile's Chrome profile belong to your user on this computer, not
to a config file: a second config naming the same profile is the same
login. Sign-ins mychart 0.4 made (a Keychain item each) move in with
`mychart profiles
migrate <NAME>`: no network, no new approval in MyChart.

## Kaiser Permanente

Kaiser's regions are Epic organizations in the directory:
`mychart orgs search kaiser`. Kaiser's front end cannot serve the MyChart page its authorize endpoint
sends the browser to (`/<region>/Authentication/OAuth/Start` answers "Page
Not Found"), so while it fails, mychart opens the same page in lowercase,
which works ([docs/api.md](docs/api.md#kaiser-permanente)).

## Docs

- [docs/registration.md](docs/registration.md): registering a household's app
- [docs/api.md](docs/api.md): Epic's behaviour and traps, with Epic's wording
- [AGENTS.md](AGENTS.md): working on the code

Licensed under MIT or Apache-2.0.
