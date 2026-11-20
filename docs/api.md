# Epic's patient-access API: what mychart relies on

Read from Epic's documentation on 2026-11-12 (fhir.epic.com renders with
JavaScript; it was read in a real browser) and checked against Epic's
public sandbox. Quotations are Epic's wording. Sources:

| Short name | Page |
|---|---|
| oauth2 | <https://fhir.epic.com/Documentation?docId=oauth2> |
| tutorial | <https://fhir.epic.com/Documentation?docId=oauth2tutorial> |
| patient apps | <https://fhir.epic.com/Documentation?docId=patientfacingfhirapps> |
| request process | <https://fhir.epic.com/Documentation?docId=epiconfhirrequestprocess> |
| guidelines | <https://fhir.epic.com/Documentation?docId=developerguidelines> |
| patient auth | <https://open.epic.com/Tutorial/PatientAuthentication> |
| endpoints | <https://open.epic.com/MyApps/Endpoints> |
| Specimen | <https://fhir.epic.com/Specifications?api=10015> (Patient Chart), <https://fhir.epic.com/Specifications?api=11366> (Outside Record) |
| Media | <https://fhir.epic.com/Specifications?api=11001> |

## 1. Registering a patient-facing app

The form ("request process"): "The app name / The app consumer type /
Applicable APIs / The default FHIR version / Applicable redirect URIs /
Whether you'll use the confidential client profile (required for refresh
tokens)". [registration.md](registration.md) has the values mychart needs.

- **Audience**: Patients ("For patients and their caretakers authenticating
  with MyChart").
- **APIs**: only those on Epic's USCDI v3 list keep automatic distribution
  (section 3). The form tags each API with the distributions it qualifies
  for (`autodownload-types`, such as `None,CMS,USCDIv1,USCDIv3`) and the
  audiences it serves (`consumer-types`). By those tags the USCDI v3 set for
  a patient app is 165 R4 APIs (2026-11-16), and it includes the two claims
  APIs, `ExplanationOfBenefit.Read (Claim)` and `.Search (Claim)`.
  `ImagingStudy` is not in it. Section 3 has why the claims APIs are
  uncertain.
- **FHIR version**: R4. `fhirUser` in the id token needs it: "The fhirUser
  claim will only be present if app has the R4 (or greater) SMART on FHIR
  version selected, and requests both the openid and fhirUser scopes"
  (oauth2).
- **Public, not confidential.** "If your application launches without SSO,
  and specifically does not include a web server component at all, then you
  can keep this unchecked" (tutorial). A public client registers no
  credential: "you will not register credentials (public keys or client
  secrets) when building your app and will configure your library to use
  client authentication type "none."" (tutorial). So no JWK Set URL.
- **Redirect URIs**: "An https protocol is required for use in a production
  environment, but http protocol-based redirect_uris are allowed for
  development and testing" (oauth2). Under "Limit Use of Localhost URIs":
  "apps should not include localhost/loopback URIs in this list. An
  exception is for Native applications which are designed to use localhost
  for the redirect", and "If your developers wish to use localhost URIs for
  testing in our sandboxes, we recommend creating a second test application
  that will never be activated in customer environments". For native apps
  "Epic recommends the use of Universal Links/App Links in lieu of custom
  redirect URL protocol schemes" (oauth2, "Considerations for Native Apps").
  The app form enforces the https rule: **Save & Ready for Production**
  stays disabled until one Endpoint URI is https ("Only secured endpoint
  URIs can be used in production.", the form's message, 2026-11-16). The docs do not
  say whether a production organization refuses an http loopback
  `redirect_uri` at authorize time, so mychart does not rely on one there.
  **What mychart sends.** By default the listener's own address,
  `http://127.0.0.1:7468/callback`: the loopback listener (cli-common's
  `pk-cli-auth`) binds and names only the IP literal (RFC 8252 §7.3). With
  `mychart config set redirect_uri https://piekstra.github.io/mychart-cli/callback`
  (mychart 0.4), it sends that page instead, for every organization.
  [registration.md](registration.md) registers the two URIs mychart sends:
  the http loopback address serves the sandbox, and the https page serves
  production. mychart never sends `localhost`, so the app need not list it. Epic compares redirect URIs exactly ("URIs are case
  sensitive", "URIs must be exact matches"), so the port is fixed (`mychart
  config set redirect_port`), never chosen at random.
  **The forwarding page.** `site/callback.html`, published from this
  repository's `main` by `.github/workflows/pages.yml`, runs
  `location.replace("http://127.0.0.1:7468/callback" + location.search)`.
  The query (`code`, `state`, `iss`, or `error`) reaches the listener
  unchanged, and the listener checks `state` and `iss` as for a direct
  redirect. The exchange names the page (pk-cli-auth v0.32's
  `AuthorizationCodeFlow::redirect_uri`, pk-med-smart v0.5's
  `SmartApp::redirect_uri`). The page sends no referrer, loads nothing, and
  runs only its own hashed script (`tests/site.rs`). Chromium applies Local
  Network Access only to iframe navigations ("Chromium only applies LNA
  restrictions to iframe navigations currently", WICG Local Network Access),
  so this top-level navigation is not gated. Headless Chromium 149 followed
  it from the live page to 127.0.0.1 with no prompt (2026-11-16).
  **What GitHub sees.** GitHub serves the page, so its logs can hold the
  one-time `code`, `state` and `iss` of each sign-in, never a token or a
  record. PKCE S256 makes a logged code useless: the verifier never leaves
  this computer, and the code is redeemed once. Only this repository's
  `main` decides what the page does.
  **The page knows one port.** It forwards to 7468, so `auth login` refuses
  (exit 2, before the browser opens) when `redirect_uri` is this page and
  `redirect_port` is not 7468. pk-cli-auth refuses a forwarded redirect
  with more or fewer than one port, since `bind_first` could otherwise take
  a port the page does not forward to. A page hosted elsewhere must forward
  to the configured port. `config set` refuses the pairing too, whichever
  key comes second, and leaves config.json as it was.
  **A missing page.** At an organization other than the sandbox with no
  `redirect_uri`, `auth login` prints a note naming the published page
  before the browser opens. mychart 0.3 reads a config.json holding
  `redirect_uri` but drops the key the next time it writes the file
  (`config set` or `config unset`), so a downgrade and upgrade lose it; the
  note shows it.
  mychart 0.1 had a `redirect_host` setting. 0.2 refuses `config set
  redirect_host` (exit 2), and `auth login` warns while config.json still
  holds a host other than 127.0.0.1, since an app registered with only that
  host's URI does not match.
- **After Ready for Production** "Neither you nor Epic can make updates to
  an app once it has been marked Ready for Production. With the exception
  of adding redirect URIs and modifying your JWK Set URLs, all technical
  changes will require you to build a new app" (request process), and an app
  "cannot be deactivated" once deployed. A patient-facing app needs a Terms
  and Conditions URL before it can be marked ready: the form's validator
  requires it ("Enter a secure URL where users can review your Terms and
  Conditions prior to authorizing access to their data.", 250 characters at
  most). The Additional Disclosure Info URL is optional ("Optional secure
  URL to share additional information about how you manage user data.").
  Neither changes after Ready for Production. This repository publishes both
  pages, at `/terms` and `/privacy` beside the forwarding page.

## 2. Refresh tokens, dynamic client registration, lifetimes

A public client gets no refresh token: "If you are using a client-side OAuth
2.0 library, you cannot use the "refresh_token" grant type for persistent
access to APIs (>1 hour of access)… If you cannot add a server component to
your application, you can add dynamic client registration to your
client-side only application and follow the directions here to use your
initial access token to get persistent access. This app is only available
for Patient-facing applications at this time." (tutorial)

What mychart does (oauth2, "Dynamic Client Registration"):

1. Standalone launch with PKCE, as a public client, scope `openid
   fhirUser`, with a fresh OpenID `nonce`. Epic's ID token carries it back:
   "The nonce claim will be present only if it was specified in the
   authorization request." pk-med-smart refuses an ID token without it, or
   with another (exit 3): nothing is stored and no client is registered.
   The access token the launch returns is the RFC
   7591 initial access token: "This token is intentionally one-time-use
   and should be used for registration immediately."
2. "Generate a public-private key pair on the user's phone or computer" and
   "Securely store that device-specific key pair": a 3072-bit RSA key, kept
   in the sign-in's vault entry with the tokens.
3. `POST` the `register` endpoint with `Authorization: Bearer <initial
   token>` and `{"software_id": <the app's client id>, "jwks": {"keys":
   [<public RSA JWK>]}}`. "software_id: This parameter must contain the
   application's client ID issued by Epic." The answer carries the dynamic
   `client_id`. Only RSA keys go inline: Epic accepts EC "only… for JWKS
   returned from a JWKS URL". pk-med-smart's body also carries RFC 7591's
   `token_endpoint_auth_method: "none"`, `grant_types: [<the JWT-bearer
   grant>]`, `response_types: []` and `redirect_uris: []`, which describe
   that client. Epic's example sends only the two above; Epic's sandbox
   accepts the other four: a sandbox sign-in (2026-11-16) registered with all
   six, and the JWT-bearer grant after it succeeded.
4. Every access token after that comes from the JWT-bearer grant (RFC 7523),
   which Epic recommends over a second launch for a refresh token:
   `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer`,
   `assertion=<JWT>`, `client_id=<dynamic id>`. The JWT's `iss` and `sub`
   are the dynamic client id, `aud` is the token endpoint, `exp` at most 5
   minutes out (mychart: 4); mychart signs RS384. No `scope` goes with it:
   Epic's example sends none, and a scope Epic did not grant could fail the
   grant.

mychart stores the sign-in, its registered client included, before the
first JWT-bearer grant. A first grant that fails leaves the registered
client in place, so the next command asks again without a new sign-in or
a second registration. A refused one shows in `auth status` until then
(below).

Lifetimes:

| What | Lifetime | Epic's wording |
|---|---|---|
| Access token | about an hour (`expires_in` 3240 to 3600 in Epic's examples) | "the number of seconds for which the access token is valid" |
| id token | 5 minutes | "This is set to the current time plus 5 minutes." |
| Dynamic client | the period the patient chose | "For the duration the user selected when approving your app… your app can get an access token whenever it needs using the JWT bearer authorization grant" |
| That period | chosen in MyChart from the organization's options, 1 hour to 1 year on Epic's screenshots; indefinite only if the organization configures it. The sandbox's consent screen (2026-11-16) offers 1 hour, 1 day, 1 week, 1 month, 3 months, 6 months and 1 year, none chosen beforehand. The authorize request has no field for it (none in Epic's documentation or the page's script), so `auth login` asks the person to choose the longest | "In patient-facing contexts, it can be set by the patient through MyChart"; "Note that this only gives patients the option of indefinite access. Each patient still chooses their max access period." |
| Without dynamic registration (`dynamic_registration false`) | the access token's hour; `renewable` false (sandbox, 2026-11-16) | "you cannot use the "refresh_token" grant type for persistent access to APIs (>1 hour of access)" |

`mychart auth status` shows each sign-in's `renewable` flag and token
expiry; when the period ends the grant fails and mychart asks for a new
`auth login` (exit 3). A token answer without `expires_in` is taken to last
an hour, as mychart 0.1 took it (Epic's answers state it), so `expires_at`
in `auth login` and `auth status` is always an RFC 3339 time and such a
token is renewed before it lapses rather than after a 401.

A server can refuse an access token before its `expires_in` is up (a 401:
revoked, or forgotten after a restart). mychart then renews it once under
the refresh lock (the JWT-bearer grant, or the refresh token when there is
no dynamic client) and asks again; another process that renewed meanwhile
has stored a different token, which is used instead. When the grant is
refused, the sign-in has no way to renew, or the server answers 401 to the
token it has just issued (and the vault still holds that token), the
stored sign-in records `refused_at` (pk-med-smart's `SmartSession`, stamped
by its `Renewer` for a refused grant and by mychart for a 401 to the token
just issued): `auth status` shows the sign-in as not
authenticated (`status.refused_at` in `--json`, a `refused_at` column in
the table), the refused token is not sent again, and the next command tries one renewal grant
before any FHIR request, which clears it on success, so a refusal that
passes does not cost a sign-in. A sign-in with no way to renew (no
dynamic client and no refresh token) stays refused, exit 3, until `auth
login`.

Each sign-in is one `SmartSession` (pk-med-smart, `format` 1) in the CLI's
vault (`pk_cli_auth::vault`): an encrypted file under
`~/.config/mychart/vault/`, entry `<profile>/<organization>/<patient>`
(`pk_med_smart::ProfileSessions`' name), sealed with one Keychain key
(`piekstra.mychart`, account `vault-key`), so a newly built mychart asks
macOS once rather than once per sign-in. A profile is one MyChart account
at one organization; every command that uses a sign-in names it
(`--profile`, or `MYCHART_PROFILE`), with no default, and its contexts are
its settings in the config (`config show` prints each profile's name and
how many patients it holds, not the patients). The vault and the browser
profiles belong to this computer's user, not to a config file: a second
config (`--config`, `MYCHART_CONFIG`) naming the same profile is the same
login, and mychart says so when a config is not beside the vault.
mychart 0.4 and earlier kept a Keychain item per sign-in, account
`smart/<organization>/<patient>`, listed in `contexts.json`. `mychart
profiles migrate <NAME> [--org <ORG>] [--patient <WHO>]` moves one
login's into a profile with pk-med-smart's `migrate_sessions` (no
network, no new approval in MyChart, one macOS dialog per item for a
newly built binary; a failure leaves the item for the next run). A login
is an organization and the user who signed in (`fhirUser`), so a parent's
own sign-in and a child's through proxy access move together; rows from
two logins are refused (exit 2, naming them) until `--org` or `--patient`
picks one, and the rows not moved stay in `contexts.json` for the next
profile. It is one-way: mychart 0.4 finds neither the moved items nor
their rows. `auth logout` removes sign-ins and never a profile
(`auth-logout/v1` `profiles_kept` is always true); `--forget` also clears
the settings other than profiles (`settings_cleared`, which 0.4 called
`config_cleared`, kept under that name too through 0.5). mychart 0.1 stored its own
shape under the same account. 0.2 does not read it: `auth status` lists it
with `status.unreadable`, any command for that patient exits 3 with the
`auth login` command that replaces it, and `auth logout` deletes it unread.
An item a newer mychart stored (a higher `format`) also exits 3, but says
to run `mychart self-update`: a sign-in would overwrite it with an older
shape.

Epic's guidelines: an app holding persistent access without a BAA "informs
the patient user of the period of time for which the app is seeking
authorization and obtains the patient user's consent" (guidelines). MyChart's
own access-period page does that; mychart prints what it got.

## 3. How a production client id reaches organizations

"Epic customers receive licenses of applications automatically on a rolling
12-hour cycle." (request process) Automatic distribution applies when the
app (patient apps):

- "Uses only USCDI v3 FHIR APIs" (Epic August 2024 and later)
- "Only reads data from Epic"
- "Is patient-facing"
- "Does not use refresh tokens OR uses refresh tokens and has a client
  credential uploaded by the vendor for that community member"
- "Selects "Enable Auto-download" and "USCDI v1" or "USCDI v3" on the app
  creation page". The page (2026-11-16) has one control for both, a radio
  "Automatic Client Distribution" with None, USCDI v1, USCDI v3 and CMS
  Patient Access API.
- "Is marked "Ready for Production""

and the organization "Has not disabled auto-download functionality". Also:
"All US provider organizations currently receive client IDs that meet the
automatic distribution criteria." There is no per-organization activation
for such an app. A sign-in at an organization that has not received the
client id yet (up to 12 hours after Ready for Production), or that turned
auto-download off, fails with an unknown-client error.

mychart's app uses no refresh tokens (the JWT-bearer grant is not one), so
it meets the condition; the docs do not say outright how a dynamic client
is counted.

**Claims and USCDI v3: Epic's lists disagree** (read 2026-11-16):

- The registration form tags `ExplanationOfBenefit.Read (Claim)` and
  `.Search (Claim)` with `USCDIv3` in `autodownload-types`, and Epic's API
  catalog (`https://fhir.epic.com/Specifications/Selections`) gives both
  `"IsUSCDI": true` and `"API_TypeName": "USCDI"`.
- The patient-facing apps guide's appendix, "In addition to the USCDI v1
  FHIR APIs above, the following FHIR APIs qualify for USCDI v3
  auto-distribution", does not name ExplanationOfBenefit. The guide puts
  it on a separate path, "Automatic Client ID Distribution: CMS Payer
  Apps", for an app that "Uses only ExplanationOfBenefit … and/or any FHIR
  APIs … from the list in the appendix" and selects "CMS Patient Access
  API", at an organization "marked to use CMS Payer APIs in their open.epic
  licensing agreement".
- Epic on the claims API: "Subject to configuration by the organization,
  this API can return resources built from pending claims, or claims
  processed or paid by an external organization. Resource information for
  these claims might be unverified, incomplete, or duplicative."

An app that selects them kept its automatic distribution. After it was
marked Ready for Production, `mychart auth probe`, which reads an
organization's answer to the authorize request without signing in
([registration.md](registration.md), step 4), found:

| Minutes after Ready for Production | Production organizations that knew the client id |
|---|---|
| 2 | 0 of 1 |
| 22 | 2 of 9 |
| 50 | 6 of 9 |

So the 12-hour cycle is an upper bound, and most organizations knew the
app within an hour. The Kaiser regions are listed by name under
[Kaiser Permanente](#kaiser-permanente). An organization that does not know
the app answers with Epic's "OAuth2 Error" page, and a sign-in there fails
until it does.

## 4. Proxy access (a parent and a child)

MyChart asks at sign-in. Epic's patient guide: "If you have access to
another person's account, you will need to select to which account you are
linking the app." Its screenshot reads "Which patient do you want
<application name> to be able to link to? Yourself / Your child" (patient
auth). One sign-in links one patient, so a parent signs in once per child
(`mychart auth login --profile <NAME> --org <ORG> --label <child>`).

The token response's `patient` is the patient picked. The id token says who
signed in: "MyChart self access workflows: Patient resource / MyChart proxy
access workflows: RelatedPerson resource" (oauth2, `sub` and `fhirUser`).
mychart records `relationship: self` when `fhirUser` is `Patient/<patient>`
and `proxy` otherwise. The proxy's RelatedPerson "is represented by a
MyChart account record ID and their link to a Patient record ID" (the
RelatedPerson (Proxy) spec).

## 5. What a patient app reads; notes and PDFs

A patient app reads the APIs it registered, of those the patient allows:
"In some cases, you may be able to select what information to share" (patient
auth). A category the patient withheld answers 403, which mychart records as a
`forbidden` skip. The USCDI v3 list is in [registration.md](registration.md).

Documents are `DocumentReference` + `Binary`:

- Clinical notes: `DocumentReference?patient=<id>&category=clinical-note`;
  "This should be set to "clinical-note" to restrict the search results to
  only include the Clinical Notes sub-resource." Each note has HTML and RTF
  attachments: "The MIME content type of the note text. This may be
  text/rtf or text/html." Scanned documents carry other types.
- Lab reports: `DocumentReference (Labs)` and `Binary.Read (Labs)`; the
  Binary is the report, "first converted to HTML" or a PDF.
- `Binary` with `Accept: application/fhir+json` "is returned as a structured
  FHIR resource response with the content Base64-encoded and returned in
  the "data" element. Alternatively, if one of those Accept header values is
  not specified, the raw content is returned directly". mychart always asks
  for the FHIR form; the stream carries the Binary resource right after the
  DocumentReference, DiagnosticReport or Media naming it.

Imaging: `ImagingStudy` is not on the USCDI v3 auto-download list, so
mychart's app does not select it. The list has `Binary (Study)`, `Media
(Study)` and `Observation (Study Finding)`: imaging reports and findings.
No API on the list serves DICOM images to a patient; images come from the
facilities (medrec).

## 6. Finding an organization

In the User-access Brands bundle (`https://open.epic.com/Endpoints/Brands`)
each organization has a brand id and an R4 base, such as
`https://<host>/FHIRPRD/api/FHIR/R4`. The deprecated R4 Endpoint bundle
(`https://open.epic.com/Endpoints/R4`) lists the same bases, some with a
trailing slash. `mychart orgs search <words>` finds an organization by its
name, its operator or a clinic's name, and `--org` takes its brand id.
Several brand ids can share one R4 base (a health system's clinics); a
sign-in at one is a sign-in at that server.

## The organization directory

"Applications SHOULD NOT perform runtime queries for endpoint data hosted
on open.epic.com. Applications should reguarly (for example, weekly)
download endpoint information" (endpoints). The Brands bundle is
a FHIR collection Bundle of about 97 MB on 2026-11-12, brand `Organization`s
(identifier system `https://open.epic.com/brand-identifier`), their
locations (`partOf`), and `Endpoint`s. mychart downloads it at most once a
week into `~/.cache/mychart/orgs.json` (an index of the primary brands with
an active endpoint) and never queries it per lookup. The server ignores
range requests, so a failed download starts over.

## Traps

- **`Epic-Client-ID` on `/metadata`.** "If you do not send the
  Epic-Client-ID HTTP header with the appropriate production or
  non-production client ID… you can receive the wrong token and authorize
  endpoints" (oauth2). The `register` endpoint appears only for a client id
  allowed to register dynamic clients; the sandbox's
  `.well-known/smart-configuration` has no `registration_endpoint` at all.
  mychart reads the CapabilityStatement with the header and falls back to
  smart-configuration.
- **`aud` is required** ("Starting in the May 2023 version of Epic, this
  parameter will be required"). mychart sends the directory's address
  untouched: the directory spells some hosts in capitals (`FHIR.KP.ORG`) and
  some bases with a trailing slash. The `aud` stays that untouched spelling;
  only FHIR requests, and the `fhir_base` that `orgs`, `patients` and the
  records stream report, use the base with its scheme and host lower-cased
  and the trailing slash removed.
- **PKCE is S256 only**: "Currently, only the S256 method is supported."
- **Standalone launch is GET**: "POST requests are supported for EHR launches
  in Epic version November 2020 and later, and are not currently supported
  for standalone launches" (oauth2).
- **Scopes**: "The scope parameter is set to "openid fhirUser"… Epic issues
  additional scopes based on the incoming APIs you configured on your
  application", and of `launch`: "Do not add it for standalone launches"
  (tutorial). mychart asks for `openid fhirUser` (`config set scope` changes
  it). `offline_access` is not needed: dynamic registration gives the
  persistent access.
- **Ids can run past 64 characters.** FHIR R4 caps an `id` at 64, but an
  app's FHIR ID generation defaults to "Use Unconstrained FHIR IDs"; the
  other choice, "Use 64-Character-Limited FHIR IDs for USCDI FHIR
  Resources", leaves other resources unlimited. Epic names no upper bound.
  mychart applies pk-med-fhir's rule (`is_id`): 1 to 256 of FHIR's id
  characters, not only dots. A sign-in granting another patient id is
  refused (exit 5, nothing kept); a read or `Binary` of another id is a
  skip. A
  document id over 100 characters is shortened in its file name to its
  first 83 and 16 hex digits of its SHA-256.
- **An empty search is not empty**: Epic answers with an `OperationOutcome`
  entry (`search.mode = outcome`, "No results were found"); mychart drops
  it from the stream.
- **Observation, Condition, CarePlan and DocumentReference split by
  category**; a category an organization does not serve answers 400, which
  mychart records as an `unsupported` skip and goes on. So does a category
  of an Observation API the client lacks: "Combination of parameters is not
  valid for any authorized sub-resource. No search was performed." mychart
  searches only the categories its registered APIs serve (`laboratory`,
  `vital-signs`, `social-history`, `survey`, `sdoh`, `imaging`,
  `care-experience-preference`); the CapabilityStatement's others
  (`core-characteristics`, `LDA`, `LDA-property`, `LDA-assessment`,
  `functional-mental-status`, `periodontal`, `labor-delivery`,
  `newborn-delivery`, `obstetrics-gynecology`) need APIs that are not on
  the USCDI v3 list.
- **A 200 can leave results out.** Beside the results, Epic adds an
  OperationOutcome entry with an issue of severity `warning` and code
  `suppressed` for each sub-resource it did not search. Epic's code 59204
  says "The authenticated client's search request applies to a
  sub-resource that the client is not authorized for. Results of this
  sub-type will not be returned." Its `diagnostics` name the sub-resource:
  "Client not authorized for Condition - Outside Record Problems. Search
  results of this type have not been included." A "User not authorized"
  issue names one the patient's account may not see ("Observation -
  SmartData Elements" in the sandbox). Epic repeats an issue in every
  search that touches the sub-resource. mychart keeps what was served and
  makes each distinct issue one `forbidden` skip per export, the shape
  medical-common's DESIGN §1.4 sets for withheld results: `resource_type`
  set, no `id` and no `search`, since what a server withholds is part of a
  type's data rather than of one search. The `reason` is the issue's text,
  after the first search that carried it and how many more did. `documents
  list` and `documents download --all` print each as a warning, and their
  `--json` output (`document-list/v1`, `document-download-batch/v1`) carries
  the same skips as `withheld`, left out when there are none (DESIGN §1.7;
  `documents/v1` has no field for it, and a reader that does not know it
  ignores it). Epic's other warnings carry no loss and are dropped:
  4101 "Resource request returns no results.", 4119 "This response includes
  information available to the authorized user at the time of the
  request…", and 59134 "Processing issues." with "Unable to build the
  document list." on a DocumentReference search without a category.
- **Outside Record APIs return nothing from Epic's November 2025
  version on**: "The Outside Record FHIR APIs are no longer enabled in the
  November 2025 version. Requests to these APIs do not return data."
  (Specimen, Outside Record). They serve what other organizations sent.
  The app registers them for organizations on an earlier version. In
  the sandbox every search reports its Outside Record sub-resource as not
  authorized, with dynamic registration and without: 27 `forbidden` skips
  per export of `fhircamila` (2026-11-16). Production organizations answer
  the same way: 27 `forbidden` skips, each "Client
  not authorized for <type> - Outside Record…" and each beside the
  organization's own results of that type, so the export is complete and
  what other organizations sent comes from signing in to them.
- **Specimen and Media cannot be searched by patient**: "This API currently
  supports only the _id search parameter. As a result, it functions
  similarly to Specimen.Read (R4). Specimen references can be returned in
  Observation (Labs) and ServiceRequest (Orders) responses." Media says the
  same: "Media references are returned in DiagnosticReport.Search (Results)
  (R4)." A search with `patient` or `subject` answers 400, "Content invalid
  against the specification or a profile." mychart reads both by reference,
  from `Observation.specimen`, `ServiceRequest.specimen` and
  `DiagnosticReport.media`, and fetches a Media's `content` Binary right
  after it.
- **What each refusal becomes** (pk-med-fhir maps the status and any
  OperationOutcome issue code; mychart picks stop or skip):

  | Answer | Exit or skip |
  |---|---|
  | 401, or an issue `login`, `unknown` or `expired`, after one renewal | stop, exit 3 |
  | 403, or an issue `forbidden`, `suppressed` or `security` | `forbidden` skip |
  | a 2xx search with a `suppressed` warning | its results kept, and one `forbidden` skip per export for each thing left out |
  | 404, 410, or an issue `not-found` or `deleted` | `not_found` skip |
  | another 4xx | `unsupported` skip |
  | 429, 5xx or an HTML 403 (an edge, not the server) after its retries; a body that is not the FHIR asked for | `upstream` skip |
  | a reference (on this server, to a followed type) whose id or version FHIR does not allow | `invalid_reference` skip, never requested; the organization's data, so the same on every export and not worth a retry |
  | no answer after its retries | stop, exit 5 |

  A search is written page by page as it is read (pk-med-fhir's
  `search_pages`). A page that fails ends that search with one skip, whose
  reason names the page and how many before it were kept; their resources
  are in the stream.
- **Next links** are absolute URLs on the server's own spelling of the host,
  which can differ in case from the directory's; mychart compares origins
  without case and follows no link to another host. An attachment URL on
  another host is a `no_file` skip; the token is not sent there.
- **References** to a Medication, Practitioner, PractitionerRole,
  Organization or Location are read after the searches, up to 2000 per
  export. A reference may be relative (`Practitioner/p1`), absolute on
  this server (`https://<base>/Practitioner/p1`, host compared without
  case), or carry a version (`Organization/o1/_history/2`, read as the
  current resource); pk-med-fhir's `ReferenceTarget` reads all three. One
  on another server, a contained one (`#x`) or a search is not followed and
  not reported. One on this server whose id or version FHIR does not allow
  is not requested: it is one `invalid_reference` skip per distinct
  reference, which names the type and the id's length, never the id.
  `records-stream/v1` skip codes are an open set, and a reader treats one
  it does not know as a skip like any other.
- **`--since`** adds `date=ge<D>` only to the searches whose type Epic
  documents a `date` parameter for: Observation, DiagnosticReport,
  DocumentReference, Encounter, Immunization and Procedure. mychart adds no
  `_lastUpdated`, which a server that does not support it can answer with a
  400 for the whole search. The Patient, the other searches and the reads by
  reference are not bounded; the stream header names their types in
  `since_unbounded`, so a reader knows they may hold older resources. A
  `Binary` comes from a dated `DocumentReference` or `DiagnosticReport`, and
  from the `content` of a `Media` read by reference, which `--since` does
  not bound.
- **Transient failures**: a request is sent at most three times when it gets
  no answer (the transport waits a minute for one), a 429, a 5xx or an HTML
  403: 1 then 2 seconds apart, or as long as `Retry-After` asks up to 30
  seconds (a longer ask is not waited for; its answer is the skip). Each
  retry prints a line on stderr. A 200 whose body is not FHIR JSON (a
  maintenance page) is an `upstream` skip; a search entry that is not a
  resource costs only itself and is counted in one `upstream` skip.
- **A `Binary`** is asked for as FHIR JSON (`data` in base64). One answered
  with another content type (the raw file) is an `upstream` skip, as is one
  over the stream's line limit (`too_large`).
- **`records export -o <DIR>`** (Epic's sandbox and servers on this machine
  only) writes `<DIR>/records.ndjson` when the stream is complete. A stream
  cut short (exit 3 or 5) goes to `<DIR>/records.incomplete.ndjson`
  instead, and `records.ndjson`, the last complete export, is left as it
  was. The `records-export/v1` result's `path` names the file written and
  its `complete` is false; the text output also names the kept file. The
  next complete export removes the incomplete one. A symbolic link at
  `records.ndjson` is refused before the Keychain or the network (exit 2).
- **The sandbox** is shared and refreshed weekly: "The Sandbox is refreshed
  every Sunday at approximately 8:00 PM Central Time". Epic does not say
  how long a new app takes to reach it. One app, checked every 30 minutes
  after **Save & Ready for Sandbox**, was unknown at 2 h 50 min and known at
  3 h 20 min. A redirect URI added to it later (the https page) was accepted
  within 32 minutes. An
  unregistered `redirect_uri` shows in the authorize answer itself: Epic's
  302 to `/mychart-fhir/Authentication/OAuth/Start` carries `error=4` in
  its query, where a registered one carries no `error`. So one GET of the
  authorize URL, with no sign-in, says whether a URI has synced.
  Developers report 12 to 24 hours. Until then its authorize page answers
  "OAuth2 Error … Something went wrong trying to authorize the client",
  which is also its answer to a production client id.
  **`mychart auth probe --org <ORG>`** reads these answers. It sends the
  request `auth login` would send (client id, redirect, scope, `aud`,
  PKCE, nonce) with one GET, follows no redirect, and opens nothing. The
  answers seen on 2026-11-16 were the same at the sandbox and in production:

  | Answer | Epic's response | `answer` |
  |---|---|---|
  | Client id unknown to the organization | 200, "OAuth2 Error" page (906 bytes at a production organization two minutes after Ready for Production, the same as for an all-zero client id) | `unknown` |
  | Client known, `redirect_uri` not on the app | 302 to `/mychart-fhir/Authentication/OAuth/Start` with `error=4` | `refused`, `error` `4` |
  | Client and redirect known | 302 to `/mychart-fhir/Authentication/OAuth/Start`, no `error` | `known` |

  Anything else is `unrecognized`. The `auth-probe/v1` document always
  carries `org`, `organization`, `client_key` (the config key whose client
  id was sent: `client_id`, or `sandbox_client_id` for the sandbox),
  `redirect_uri`, `answer` and `status` (the HTTP status); `error` only with
  `refused`. A known client's
  redirect leads to MyChart's sign-in, which the probe does not follow, so
  no `code` is ever issued to it. Its test patients (`fhircamila`, `fhirderrick`,
  `fhirdesiree`, password `epicepic1`) are synthetic and published by Epic.

## Kaiser Permanente

Kaiser's clinical records are Epic. Kaiser's developer portal
(<https://developer.kp.org/#/apis>) lists its clinical API as "Other
Interoperability": "For Clinical Data exchange Kaiser Permanente use Epic on
FHIR." Its clinical developer guide (June 2023, linked from that entry):
"No approval (beyond the OAuth security flow) is needed for the open.epic
API" and "There is no testing with Kaiser. All testing is done with Epic in
their Testing Sandbox." So each region is an `--org` of mychart, signed in
separately.

| Brand (Brands bundle id) | R4 base | Authorization server number |
|---|---|---|
| Kaiser Permanente - Colorado (478) | `.../esb-envlbl/140/api/FHIR/R4/` | 140 |
| Kaiser Permanente - Georgia (237) | `.../esb-envlbl/200/api/FHIR/R4/` | 200 |
| Kaiser Permanente - Hawaii (479) | `.../esb-envlbl/130/api/FHIR/R4/` | 130 |
| Kaiser Permanente - Mid-Atlantic (480) | `https://FHIR.KP.ORG/.../esb-envlbl/170/api/FHIR/R4` | 170 |
| Kaiser Permanente - Northern California (482) | `https://FHIR.KP.ORG/.../esb-envlbl/326/api/FHIR/R4/` | 326 |
| Kaiser Permanente - Northwest (132) | `https://FHIR.KP.ORG/.../esb-envlbl/190/api/FHIR/R4/` | 190 |
| Kaiser Permanente - Southern California (483) | `.../esb-envlbl/226/api/FHIR/R4/` | 226 |
| Kaiser Permanente - Washington (439) | `https://fhir.kp.org/Interconnect-FHIR-PRD/api/FHIR/R4` | 750 |

`...` is `https://fhir.kp.org/service/ptnt_care/EpicEdiFhirRoutingSvc/v2014`.
The authorization server is
`https://fhir.kp.org/KPPolarisPortal/esb-envlbl/<number>/oauth2`, off the
base's path, which discovery finds; never derive it from the base. The
deprecated R4 bundle still lists Southern California at 212 and Northern
California at 312 plus a second entry ("Kaiser – NCAL - prod 326 (CIS)");
those bases authorize at 226 and 326. mychart uses Brands.

- **Discovery** at every base (read 2026-11-12) is identical apart from its
  URLs: `client-public`, `context-standalone-patient`, `permission-offline`,
  `permission-v1` and `-v2`, PKCE `S256`, and the `jwt-bearer` grant. The
  CapabilityStatement's `implementation.url` is the `KPPolarisPortal` path,
  not the base; requests and `aud` use the directory's base.
- **Sign-in: Kaiser's Start page fails in its own capitals.** At a region
  other than Washington, the authorize endpoint answers a 302 to
  `https://healthy.kaiserpermanente.org:443/<region>/Authentication/OAuth/Start?<the
  authorize parameters>`, where `<region>` is the region's MyChart path. Kaiser's front end answers that path 500, "Kaiser
  Permanente - Page Not Found", with or without parameters and with or
  without a kp.org session (2026-11-19). The same path in lowercase,
  `/<region>/authentication/oauth/start` with the same query, reaches Epic:
  302 to `/<region>/Home/Logout`, the first step of Epic's sign-in, as in the
  sandbox. A developer of another patient app reports the same for every
  region whose authorize endpoint sends the browser there (all but
  Washington), with real sign-ins completed through the lowercase path in
  Northern and Southern California
  (<https://github.com/EnriqueNeyra/syntropy-health/pull/22>, 2026-10-03).
  So where the authorization endpoint is on `fhir.kp.org`, `auth login`
  sends the authorize request itself first, its redirect not followed, as
  `auth probe` does (no `code` is issued without a session). When the
  answer is that Start page and the page as it is, without its query,
  still fails, the window opens the lowercase address instead of the
  authorize address (`epic::address_to_open`). The page counts as fixed
  only when it answers below 400, so a 403 or 429 is not taken for a fix.
  Any other answer, a failed authorize request, or a fixed page opens the
  authorize address as before. Each check waits 5 seconds at most, and
  `auth login` says on stderr that it is checking and when it opens the
  lowercase address.
- **Kaiser's sign-in comes from the Start page itself.** Followed without
  cookies (2026-11-19), the lowercase Start page with its query goes
  through `/<region>/Home/Logout`, `/<region>/InsideAsp/Mode`,
  `/<region>/Authentication/Login` and `/<region>/Authentication/Saml/Login`
  to Kaiser's own sign-in, `https://identityauth.kaiserpermanente.org/idp/SSO.saml2`
  ("Sign in | Kaiser Permanente"), which returns to MyChart by SAML. So the
  window opens the sign-in's address alone. mychart 0.5.0 and 0.5.1 opened
  kp.org's sign-in (`https://healthy.kaiserpermanente.org/sign-on`) in a tab
  before it, from the earlier reading that the Start page needs a kp.org
  session. That tab reached kp.org's own sign-in on the same host
  (`identityauth.kaiserpermanente.org/as/authorization.oauth2`, also "Sign
  in | Kaiser Permanente"), so the window showed two sign-in pages, and
  0.5.2 dropped it. The profile's Chrome profile keeps Kaiser's
  sign-in cookies either way. The MyChart login page
  `/<region>/Authentication/Login` answers 410 Gone when opened on its own
  (2026-11-17) and forwards to the SAML sign-in from inside the chain above:
  Kaiser members sign in at kp.org ("Since MyChart is already built into
  kp.org and the Kaiser Permanente app, there's no need for you to create a
  separate MyChart account"). Washington (439), the former Group Health, has
  a MyChart login page of its own.
- **Paging links in capitals.** The same report says Kaiser writes its
  server's name in capitals in `next` links (`FHIR.KP.ORG`) for a base
  listed as `fhir.kp.org`. mychart and pk-med-fhir compare origins with the
  scheme and host lowercased, so those links are followed.
- **Northwest and Washington are two organizations.** Northwest covers Oregon
  and SW Washington; Washington is the former Group Health. A person with
  records in both signs in to each.
- **Coverage** comes through mychart: Kaiser's health plan runs in the same
  Epic instance, and `Coverage` is on the USCDI v3 list. **Claims**
  (`ExplanationOfBenefit`): Kaiser's servers list EOB, and the app selects
  it (section 1), so `records export` asks for it; what Kaiser's Epic
  answers is not yet seen. Kaiser also serves
  claims from a separate health-plan Patient Access API at
  `kpx-service-bus.kp.org`, which needs its own reviewed registration at
  developer.kp.org; Kaiser: "The health plan API (Patient Access API) is
  separate and distinct from the open.epic clinical data API", with separate
  member consent. mychart does not call it.
- **Proxy.** Kaiser's "Act for a Family Member" gives a parent kp.org access
  to a dependent under 18 (the child's name, birth date and record number; a
  security code mailed home, or set up in person). Whether the proxied child
  then appears in the patient picker of a standalone launch at Kaiser shows
  on the first proxy sign-in.
- **Images.** Kaiser's servers list `ImagingStudy`, but the app does not
  select it (section 5). No Kaiser API serves DICOM to a patient; studies
  come by a release-of-information request to the region. Washington:
  "Copies of digital images are available on CD", and a CD not handed to the
  patient in person "will be encrypted". Northwest: an Imaging ROI request;
  dental and eye images through a download portal (Nuance PowerShare) with a
  DICOM download.
- **Distribution.** Epic distributes a qualifying app's client id unless an
  organization "has disabled auto-download functionality"; Kaiser does not
  say whether it has. After an app was marked Ready for Production,
  `mychart auth probe` read:

  | Region (brand id) | 22 minutes on | 50 minutes on |
  |---|---|---|
  | Northwest (132) | known | known |
  | Mid-Atlantic (480) | known | known |
  | Georgia (237) | unknown | known |
  | Northern California (482) | unknown | known |
  | Southern California (483) | unknown | known |
  | Colorado (478) | unknown | unknown |
  | Hawaii (479) | unknown | unknown |
  | Washington (439) | unknown | unknown |

  So Kaiser has not turned auto-download off in five regions. For the
  other three, an `unknown` 24 hours after Ready for Production, twice
  Epic's 12-hour cycle and the margin registration.md step 4 uses, would
  mean it has.
  `mychart auth probe --org <ID>` answers it again without signing in.
