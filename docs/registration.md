# Registering your app with Epic

`mychart` ships without a client id. Each person or household that uses it
registers its own patient-facing app at [fhir.epic.com](https://fhir.epic.com)
and sets the two client ids Epic gives. The registration is free, needs no
Epic customer, and takes about 20 minutes, then a few hours for Epic to
sync. `mychart registration` prints every field below with its value (`--json`
for `registration/v1`; `--apis` lists the APIs to select). Epic's wording for each step
is quoted in [api.md](api.md).

A public client's client id is not a secret: it travels in every sign-in
URL, and Epic tells developers to hand client ids to health systems. Epic's
App Developer Guidelines still recommend that "The app keeps client
identifiers confidential and does not disclose them to any other party or
use them for any other purpose" (section 2, Security). So this repository
ships none; keep yours to your own config.

## What you are registering

| Setting | mychart's |
| --- | --- |
| Launch | Standalone patient launch: the person starts it, not an EHR |
| Client type | Public (no secret), with Epic's dynamic client registration: each install makes its own key and gets access tokens with the JWT-bearer grant |
| PKCE | S256, on every sign-in |
| Redirect URIs | `http://127.0.0.1:7468/callback`, where mychart listens, and the https page `https://piekstra.github.io/mychart-cli/callback`, which forwards a sign-in to it (production takes only https). Another `redirect_port` changes the first; `mychart registration` prints them for yours. mychart never sends `localhost` |
| Scopes | SMART v1; mychart asks for `openid fhirUser`, and Epic adds the scopes of the APIs selected |
| APIs | 165, all read-only, all on Epic's USCDI v3 list, so the client id reaches every US organization by itself |

## 1. Create a developer account

1. Open <https://fhir.epic.com> and choose **Sign Up**.
2. Fill in the sign-up page as an individual: a person or household
   registering its own app is not a company and not a healthcare
   organization. Fields
   marked `*` are required.

   | Section | Field | What to enter |
   | --- | --- | --- |
   | User Information | First Name\*, Last Name\* | Your own. |
   | Business Information | Company Email\* | Your own email address. |
   | | Phone | Optional: your own, or empty. |
   | | Company Name\* | Your own name. Epic's public pages do not say where this name appears; assume a patient signing in may see it. |
   | | Website | Optional. Choose `https://` and enter the repository of the mychart you run: `github.com/piekstra/mychart-cli`. |
   | | Country\*, Address\*, City\*, State\*, Zip Code\* | Your own address, for Epic. |

   Complete the reCAPTCHA and choose **Submit**. The page asks nothing
   about funding or data sharing; the app's questionnaire does, in step 2.
3. Confirm the address from the email Epic sends.
4. Accept the open.epic Terms of Use when asked. If Epic shows the account
   as awaiting review, wait for its email before signing in to create the
   app.

## 2. Create the app

On **Build Apps**, choose **Create** (or **Create My First App**). The app is
made in two pages: **Create**, then **Edit**, which opens after the first
save.

### The Create page

| Field | Value |
| --- | --- |
| Application Name | A name you recognise, for example `My Records (mychart-cli)`. MyChart shows it on the sign-in consent page ("Which patient do you want <application name> to be able to link to?"). |
| Application Audience | **Patients** ("For patients and their caretakers authenticating with MyChart"; the others are Clinicians or Administrative Users, and Backend Systems). Leave any use-case radios the page shows at their default. |
| Automatic Client Distribution | **USCDI v3** (the others are None, USCDI v1 and CMS Patient Access API). The page then says "This app will be automatically downloaded to all qualifying customers upon marking it ready for production." |
| Public Documentation URL | Choose `https://` in the scheme list (the default is `http://`) and enter `github.com/piekstra/mychart-cli`: its README says what the app reads, where records go, that it only reads, and how to end its access. |
| Incoming APIs | Every API in the [list below](#apis-to-select), moved from **Available** to **Selected** with `>>`; the **Search available service list** box filters the list. |
| Outgoing APIs | None. |
| Endpoint URI | The redirect URIs, one row each (**Add Another URI** adds a row), the scheme from the row's scheme list and the rest without a scheme. Row 1: `http://` and `127.0.0.1:7468/callback`. Row 2: `https://` and `piekstra.github.io/mychart-cli/callback`, the page that forwards a sign-in to this computer ([api.md](api.md), section 1): production takes only https, and **Save & Ready for Production** stays disabled until an https row exists. Epic compares them character for character. |
| Can Register Dynamic Clients ("Will this app register dynamic clients?") | **Checked**. mychart has no server to keep a secret; each install registers its own key instead. |
| "How will dynamic clients request an access token?" | **JWT Bearer Grant Type** (the default). |
| Is Confidential Client | **Unchecked**. Its extra settings (Requires Persistent Access, Uses Rolling Refresh Tokens, Can Have Indefinite Access, JWK Set URLs) appear only when it is checked. |

Choose **Save**. The page moves to the app's **Edit** page, with a progress
bar (Created, Test, Ready).

### The Edit page

| Field | Value |
| --- | --- |
| Client ID, Non-Production Client ID | Shown, not editable. |
| FHIR version | **R4** (the default). |
| SMART scope version | **SMART v1** (the default). mychart asks for `openid fhirUser`; Epic adds the scopes of the selected APIs. |
| FHIR ID generation | **Use Unconstrained FHIR IDs** (the default). Ids can then run past FHIR's 64 characters; mychart accepts them. The other choice, "Use 64-Character-Limited FHIR IDs for USCDI FHIR Resources", does not limit every resource. |
| Summary (500 characters at most) | `A command-line tool that downloads a person's own records, and their children's through proxy access, from MyChart through Epic's patient-access FHIR API. It only reads. Records stay on the person's computer.` |
| Description (2000 characters at most) | What mychart does, in a paragraph: it downloads the person's own records, only reads, keeps them on their computer, signs in through a browser window of its own, and loses access when the person removes it under MyChart's linked apps or runs `mychart auth logout`. |
| Intended Purposes | **Individuals' Access to their EHI** only. |
| Intended Users | **Individual/Caregiver** only. |
| Terms and Conditions URL | Required for **Save & Ready for Production** on a patient-facing app. Choose `https://` and enter `piekstra.github.io/mychart-cli/terms`, the Terms of Use this repository publishes, or a page of your own. It cannot change after Ready for Production. |
| Additional Disclosure Info URL | Optional. Choose `https://` and enter `piekstra.github.io/mychart-cli/privacy`, or a page of your own. It cannot change after Ready for Production. |
| Thumbnail, Screenshots | Leave empty. |
| Request Connection Hub Listing | Not used. |
| "I accept the terms of use of open.epic." | Tick it before each save; it shows unticked again after the page reloads. |

Choose **Save & Ready for Sandbox**. **My Apps** lists the app as
**Draft**. Do **not** choose **Save & Ready for Production** yet: after that,
nothing but redirect URIs and JWK Set URLs can change, and a mistake means a
new app.

Save before opening the Data Use Questionnaire: **Open Data Use
Questionnaire** leaves the Edit page, and edits not yet saved there are
lost.

### The Data Use Questionnaire

**Open Data Use Questionnaire** on the Edit page. Choose the button "I
understand that customers will have access to my responses and that my
answers will be shown to users of my app." Two tabs appear,
"Questionnaire from 5/28/2021" and "Questionnaire from 12/27/2018"; answer
both. Each question has its own **Save Changes**. Some questions appear only
after an earlier answer: "No one; data never leaves the user's device" hides
the 2018 questions on approval, notification and obtaining a record, and
storing data locally adds "For how long does this app store user data?" and
"Does this app allow users to delete all of the data that have been stored
about them?". MyChart shows the answers on the consent page, and a warning
when they are missing.

What mychart truthfully answers:

| Question (in short) | Answer |
| --- | --- |
| Who develops the app | An individual developer |
| Business associate agreement | None |
| Source | Open source |
| Where user data is stored | Locally, on the user's device |
| Who else can access it | No one |
| Whether the complete record is available to the user | Yes |
| Whether the user can delete all of it | Yes |
| What is kept after the app is deleted | Nothing |
| Other uses of the data | None |
| Other individuals' data | None |
| How long data is stored | Indefinitely: until the user deletes the files |
| Access log | None |

Do **not** choose **Lock my answers**: locking makes them read-only and
revokes every token the app holds within 0 to 12 hours. They become
read-only at Ready for Production anyway.

## 3. Test in Epic's sandbox

Set the sandbox's client id first:

```sh
mychart config set sandbox_client_id <Non-Production Client ID>
```

Epic does not say how long a new app takes to reach its sandbox. One app
reached it about 2 h 50 min to 3 h 20 min after **Save & Ready for
Sandbox**; developers report 12 to 24 hours. To check
without signing in, run `mychart auth probe --org sandbox`. It answers
`known`, `unknown` (the sandbox does not know the client yet), or `refused`
with Epic's error code (`4`: the app does not list the redirect). Then:

```sh
mychart profiles add test-sandbox
export MYCHART_PROFILE=test-sandbox
mychart auth login --org sandbox --label test
mychart patients list
mychart records export --org sandbox --patient test --ndjson | head -c 300
```

A profile is one MyChart account at one organization; the sign-in opens
in that profile's own Chrome window.

Sign in with one of Epic's published test patients (`fhircamila`,
`fhirderrick` or `fhirdesiree`, password `epicepic1`; synthetic people). Put
only synthetic data in the sandbox.

`mychart auth login` reports whether the organization offered dynamic client
registration. If it did not, the sign-in lasts an hour and the app's
**Can Register Dynamic Clients** setting needs checking before the next step.

## 4. Mark it Ready for Production

Open the app on **Build Apps**, tick "I accept the terms of use of
open.epic.", and choose **Save & Ready for Production**. Epic distributes the client id to every US organization that has not turned
off auto-download, "on a rolling 12-hour cycle". An app deployed to
production "cannot be deactivated", only deactivated by each organization.

```sh
mychart config set client_id <Client ID>
mychart config set redirect_uri https://piekstra.github.io/mychart-cli/callback
mychart orgs search "<your health system>"
mychart profiles add <NAME>
mychart auth login --profile <NAME> --org <ID> --label <WHO>
```

`redirect_uri` makes every sign-in send the https page, which production
needs; the sandbox accepts it too once Epic has synced the app.
`mychart config unset redirect_uri` goes back to the loopback address.

A sign-in at an organization within 12 hours of marking the app ready can
fail with an unknown-client error; wait and try again. One app reached 6
of 9 organizations it was checked against within 50 minutes ([api.md](api.md)
section 3).

To check that the client id reached an organization without signing in,
run `mychart auth probe --org <ID>`: `known` means a sign-in can start,
`unknown` that the organization does not know the client id yet. If it
still answers `unknown` 24 hours after Ready for Production, the claims APIs
(above) are the first suspect: a new app without them is the remedy, since
an app's APIs cannot change after production.

## What the portal refuses, and the way around it

- **An http redirect in production.** Production organizations take only
  https redirect URIs, and **Save & Ready for Production** stays disabled
  until an https row exists. mychart listens on `127.0.0.1`, so the app
  also lists `https://piekstra.github.io/mychart-cli/callback`, a page that
  passes the sign-in's answer on to the listener, and `mychart config set
  redirect_uri` makes every sign-in send it ([api.md](api.md), section 1).
- **`http://` by default.** Each URL field has a scheme list that starts
  at `http://`; choose `https://` before typing the rest without a scheme.
- **No Terms and Conditions URL.** A patient-facing app cannot be marked
  ready without one. This repository publishes
  `https://piekstra.github.io/mychart-cli/terms`, and `/privacy` beside it.
- **Changes after Ready for Production.** Only redirect URIs and JWK Set
  URLs can change; anything else needs a new app, and a production app
  "cannot be deactivated". Test in the sandbox first.
- **The terms checkbox unticks itself** after each reload; tick it before
  every save.
- **Unsaved edits** are lost when **Open Data Use Questionnaire** leaves
  the Edit page; save first.
- **Lock my answers** revokes every token the app holds within 0 to 12
  hours; leave it.
- **An unknown client** for up to 12 hours after Ready for Production:
  `mychart auth probe --org <ID>` says when an organization knows it.

## The terms' key points

Read 2026-11-20 from the open.epic Terms of Use
(<https://fhir.epic.com/Resources/Terms>), Epic's App Developer Guidelines
(<https://fhir.epic.com/Documentation?docId=developerguidelines>), its FAQ
and its patient authentication tutorial. These are the points, not the
terms; read them yourself before you accept.

- **You are responsible.** "As between you and Epic, you are solely
  responsible for your products and services … and for all liability and
  consequences (e.g., Claims by or on behalf of patients … privacy
  breaches; and security vulnerabilities)", and you "agree to indemnify,
  hold harmless and defend Epic".
- **Security.** A security vulnerability you find in Epic's software is
  reported to Epic "as soon as possible".
- **Reporting.** Epic "must measure and report information about
  applications that connect to our certified software": names, developers,
  intended purposes and users, and whether they are used, and may share it.
- **The guidelines** are recommendations ("you and your apps are strongly
  encouraged to practice the App Developer Guidelines"). Among them: keep
  client identifiers confidential; give patients a clear data use or
  privacy policy (this repository's `/privacy` page); keep the Data Use
  Questionnaire's answers true, by a new app if they change; and use no
  more than 1% of an organization's database resources at peak.
- **What patients see.** MyChart's consent page reads "<application
  name>, provided by <application developer>, is not affiliated with your
  healthcare provider. <application developer> is not obligated by HIPAA
  privacy guidelines to protect your health information", then the
  questionnaire's answers; with none, it warns "Privacy Risk".
- **Cost and review.** "There are no licensing costs to the app
  developer", and "Epic does not review, certify, or verify developers …
  or their applications."
- **Epic's materials** (sandboxes, documentation, specifications) may be
  shared "only by linking others directly to the Materials on the website".

## APIs to select

Every R4 API on the form whose `autodownload-types` include `USCDIv3` and
whose `consumer-types` include `Patient`: 165 APIs, in two groups. The form
keeps both lists in its markup; this list is that set as the form names it
(2026-11-16). Nothing can be added after **Save & Ready for Production**, so
select both groups.

The two claims APIs, `ExplanationOfBenefit.Read (Claim)` and
`ExplanationOfBenefit.Search (Claim)`, are in the set, and Epic's own lists
disagree about them: the form and Epic's API catalog count them as USCDI,
while the appendix of Epic's patient-facing apps guide does not name them
([api.md](api.md), section 3). They are selected. Whether the app still
reaches organizations by itself shows only after **Save & Ready for
Production**; step 4 says how to check. `ImagingStudy` is not in the set,
and is left out.

### Records the organization keeps (95)

- AllergyIntolerance.Read (Patient Chart) (R4)
- AllergyIntolerance.Search (Patient Chart) (R4)
- Binary.Read (Clinical Notes) (R4)
- Binary.Read (Generated CDAs) (R4)
- Binary.Read (Labs) (R4)
- Binary.Read (Study) (R4)
- Binary.Search (Clinical Notes) (R4)
- Binary.Search (Generated CDAs) (R4)
- Binary.Search (Labs) (R4)
- Binary.Search (Study) (R4)
- CarePlan.Read (Encounter) (R4)
- CarePlan.Read (Longitudinal) (R4)
- CarePlan.Search (Encounter) (R4)
- CarePlan.Search (Longitudinal) (R4)
- CareTeam.Read (Longitudinal CareTeam) (R4)
- CareTeam.Search (Longitudinal CareTeam) (R4)
- Condition.Read (Care Plan Problem) (R4)
- Condition.Read (Encounter Diagnosis) (R4)
- Condition.Read (Health Concerns) (R4)
- Condition.Read (Problems) (R4)
- Condition.Search (Care Plan Problem) (R4)
- Condition.Search (Encounter Diagnosis) (R4)
- Condition.Search (Health Concerns) (R4)
- Condition.Search (Problems) (R4)
- Coverage.Read (Patient Insurance Information) (R4)
- Coverage.Search (Patient Insurance Information) (R4)
- Device.Read (Implants) (R4)
- Device.Search (Implants) (R4)
- DiagnosticReport.Read (Results) (R4)
- DiagnosticReport.Search (Results) (R4)
- DocumentReference.Read (Clinical Notes) (R4)
- DocumentReference.Read (Generated CDAs) (R4)
- DocumentReference.Read (Labs) (R4)
- DocumentReference.Search (Clinical Notes) (R4)
- DocumentReference.Search (Generated CDAs) (R4)
- DocumentReference.Search (Labs) (R4)
- Encounter.Read (Patient Chart) (R4)
- Encounter.Search (Patient Chart) (R4)
- ExplanationOfBenefit.Read (Claim) (R4)
- ExplanationOfBenefit.Search (Claim) (R4)
- Goal.Read (Care Plan Goal) (R4)
- Goal.Read (Patient) (R4)
- Goal.Search (Care Plan Goal) (R4)
- Goal.Search (Patient) (R4)
- Immunization.Read (Patient Chart) (R4)
- Immunization.Search (Patient Chart) (R4)
- Location.Read (Organizational Directory) (R4)
- Location.Search (Organizational Directory) (R4)
- Media.Read (Study) (R4)
- Media.Search (Study) (R4)
- Medication.Read (Organization Med List) (R4)
- Medication.Search (Organization Med List) (R4)
- MedicationDispense.Read (Fill Status) (R4)
- MedicationDispense.Search (Fill Status) (R4)
- MedicationRequest.Read (Signed Medication Order) (R4)
- MedicationRequest.Search (Signed Medication Order) (R4)
- Observation.Read (Assessments) (R4)
- Observation.Read (Labs) (R4)
- Observation.Read (SDOH Assessments) (R4)
- Observation.Read (SmartData Elements) (R4)
- Observation.Read (Social History) (R4)
- Observation.Read (Study Finding) (R4)
- Observation.Read (Vital Signs) (R4)
- Observation.Search (Assessments) (R4)
- Observation.Search (Labs) (R4)
- Observation.Search (SDOH Assessments) (R4)
- Observation.Search (SmartData Elements) (R4)
- Observation.Search (Social History) (R4)
- Observation.Search (Study Finding) (R4)
- Observation.Search (Vital Signs) (R4)
- Organization.Read (Organizational Directory) (R4)
- Organization.Search (Organizational Directory) (R4)
- Patient.Read (Demographics) (R4)
- Patient.Search (Demographics) (R4)
- Practitioner.Read (Organizational Directory) (R4)
- Practitioner.Search (Organizational Directory) (R4)
- PractitionerRole.Read (Organizational Directory) (R4)
- PractitionerRole.Search (Organizational Directory) (R4)
- Procedure.Read (Orders) (R4)
- Procedure.Read (SDOH Intervention) (R4)
- Procedure.Read (Surgeries) (R4)
- Procedure.Search (Orders) (R4)
- Procedure.Search (SDOH Intervention) (R4)
- Procedure.Search (Surgeries) (R4)
- Provenance.Read (R4)
- RelatedPerson.Read (Friends and Family) (R4)
- RelatedPerson.Read (Proxy) (R4)
- RelatedPerson.Search (Friends and Family) (R4)
- RelatedPerson.Search (Proxy) (R4)
- ServiceRequest.Read (Community Resource ServiceRequest) (R4)
- ServiceRequest.Read (Orders) (R4)
- ServiceRequest.Search (Community Resource ServiceRequest) (R4)
- ServiceRequest.Search (Orders) (R4)
- Specimen.Read (Patient Chart) (R4)
- Specimen.Search (Patient Chart) (R4)

### Copies other organizations sent ("Outside Record", 70)

- AllergyIntolerance.Read (Outside Record) (R4)
- AllergyIntolerance.Search (Outside Record) (R4)
- Binary.Read (Outside Record - Clinical Notes) (R4)
- Binary.Search (Outside Record - Clinical Notes) (R4)
- CarePlan.Read (Outside Record) (R4)
- CarePlan.Search (Outside Record) (R4)
- CareTeam.Read (Outside Record) (R4)
- CareTeam.Search (Outside Record) (R4)
- Condition.Read (Outside Record Encounter Diagnosis) (R4)
- Condition.Read (Outside Record Health Concern) (R4)
- Condition.Read (Outside Record Problems) (R4)
- Condition.Search (Outside Record Encounter Diagnosis) (R4)
- Condition.Search (Outside Record Health Concern) (R4)
- Condition.Search (Outside Record Problems) (R4)
- Coverage.Read (Outside Record) (R4)
- Coverage.Search (Outside Record) (R4)
- Device.Read (Outside Record) (R4)
- Device.Search (Outside Record) (R4)
- DiagnosticReport.Read (Outside Record Results) (R4)
- DiagnosticReport.Search (Outside Record Results) (R4)
- DocumentReference.Read (Outside Record - Clinical Notes) (R4)
- DocumentReference.Search (Outside Record - Clinical Notes) (R4)
- Encounter.Read (Outside Record) (R4)
- Encounter.Search (Outside Record) (R4)
- Goal.Read (Outside Record) (R4)
- Goal.Search (Outside Record) (R4)
- Immunization.Read (Outside Record) (R4)
- Immunization.Search (Outside Record) (R4)
- Location.Read (Outside Record) (R4)
- Location.Search (Outside Record) (R4)
- Medication.Read (Outside Record) (R4)
- Medication.Search (Outside Record) (R4)
- MedicationDispense.Read (Outside Record) (R4)
- MedicationDispense.Search (Outside Record) (R4)
- MedicationRequest.Read (Outside Record) (R4)
- MedicationRequest.Search (Outside Record) (R4)
- Observation.Read (Outside Record Activities of Daily Living) (R4)
- Observation.Read (Outside Record Occupation) (R4)
- Observation.Read (Outside Record Pregnancy Status) (R4)
- Observation.Read (Outside Record Results) (R4)
- Observation.Read (Outside Record Screening Assessment) (R4)
- Observation.Read (Outside Record SDOH Assessment) (R4)
- Observation.Read (Outside Record Sexual Orientation) (R4)
- Observation.Read (Outside Record Smoking Status) (R4)
- Observation.Read (Outside Record Vital Signs) (R4)
- Observation.Search (Outside Record Activities of Daily Living) (R4)
- Observation.Search (Outside Record Occupation) (R4)
- Observation.Search (Outside Record Pregnancy Status) (R4)
- Observation.Search (Outside Record Results) (R4)
- Observation.Search (Outside Record Screening Assessment) (R4)
- Observation.Search (Outside Record SDOH Assessment) (R4)
- Observation.Search (Outside Record Sexual Orientation) (R4)
- Observation.Search (Outside Record Smoking Status) (R4)
- Observation.Search (Outside Record Vital Signs) (R4)
- Organization.Read (Outside Record) (R4)
- Organization.Search (Outside Record) (R4)
- Practitioner.Read (Outside Record) (R4)
- Practitioner.Search (Outside Record) (R4)
- PractitionerRole.Read (Outside Record) (R4)
- PractitionerRole.Search (Outside Record) (R4)
- Procedure.Read (Outside Record) (R4)
- Procedure.Search (Outside Record) (R4)
- QuestionnaireResponse.Read (Outside Record) (R4)
- QuestionnaireResponse.Search (Outside Record) (R4)
- RelatedPerson.Read (Outside Record) (R4)
- RelatedPerson.Search (Outside Record) (R4)
- ServiceRequest.Read (Outside Record) (R4)
- ServiceRequest.Search (Outside Record) (R4)
- Specimen.Read (Outside Record) (R4)
- Specimen.Search (Outside Record) (R4)
