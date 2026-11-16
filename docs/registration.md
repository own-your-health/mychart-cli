# Registering your household's app with Epic

`mychart` ships without a client id. Each household registers its own
patient-facing app at [fhir.epic.com](https://fhir.epic.com) and sets the two
client ids it is given. The registration is free, needs no Epic customer, and
takes about 20 minutes. Epic's wording for each step is quoted in
[api.md](api.md).

A client id of a public client is not a secret: it travels in every sign-in
URL. Epic's guidelines still ask developers to keep client identifiers
confidential, so do not publish yours.

## 1. Create a developer account

1. Open <https://fhir.epic.com> and choose **Sign Up**.
2. Fill in the sign-up page as an individual: a household registering its
   own app is not a company and not a healthcare organization. Fields
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
| Application Name | A name your household recognises, for example `Household Records (mychart-cli)`. MyChart shows it on the sign-in consent page ("Which patient do you want <application name> to be able to link to?"). |
| Application Audience | **Patients** ("For patients and their caretakers authenticating with MyChart"; the others are Clinicians or Administrative Users, and Backend Systems). Leave any use-case radios the page shows at their default. |
| Automatic Client Distribution | **USCDI v3** (the others are None, USCDI v1 and CMS Patient Access API). The page then says "This app will be automatically downloaded to all qualifying customers upon marking it ready for production." |
| Public Documentation URL | Choose `https://` in the scheme list (the default is `http://`) and enter `github.com/piekstra/mychart-cli`: its README says what the app reads, where records go, that it only reads, and how to end its access. |
| Incoming APIs | Every API in the [list below](#apis-to-select), moved from **Available** to **Selected** with `>>`; the **Search available service list** box filters the list. |
| Outgoing APIs | None. |
| Endpoint URI | The redirect URIs, one row each (**Add Another URI** adds a row). Choose `http://` in the row's scheme list and enter the rest without a scheme: `127.0.0.1:7468/callback`, and in a second row `localhost:7468/callback`. Epic compares them character for character. |
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
| Summary (500 characters at most) | `A command-line tool a household uses to download its members' own records from MyChart through Epic's patient-access FHIR API. It only reads. Records stay on the household's computer.` |
| Description (2000 characters at most) | What mychart does, in a paragraph: it downloads the household's own records, only reads, keeps them on the household's computer, signs in through the system browser, and loses access when the person removes it under MyChart's linked apps or runs `mychart auth logout`. |
| Intended Purposes | **Individuals' Access to their EHI** only. |
| Intended Users | **Individual/Caregiver** only. |
| Thumbnail, Screenshots, Terms and Conditions URL, Additional Disclosure Info URL | Leave empty. |
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

Epic does not say how long a new app takes to reach its sandbox. The
household's app was still unknown there 85 minutes after **Save & Ready for
Sandbox** (2026-11-16), and developers report 12 to 24 hours. To check
without signing in, run `mychart auth login --org sandbox --no-browser`,
open the URL it prints, and end the command with Ctrl-C. Epic's "OAuth2
Error" page ("Something went wrong trying to authorize the client") means
the sandbox does not know the client yet; a MyChart sign-in page means it
does. Then:

```sh
mychart auth login --org sandbox --label test
mychart patients list
mychart records export --org sandbox --patient test --ndjson | head -c 300
```

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
mychart orgs search "<your health system>"
mychart auth login --org <ID> --label <WHO>
```

A sign-in at an organization within 12 hours of marking the app ready can
fail with an unknown-client error; wait and try again.

To check that the client id reached an organization without signing in,
open the authorize URL that `mychart auth login --org <ID> --no-browser`
prints and close it at the next page: the organization's MyChart sign-in
page means it knows the app, and an OAuth error page means it does not. If
it still does not 24 hours after Ready for Production, the claims APIs
(above) are the first suspect: a new app without them is the remedy, since
an app's APIs cannot change after production.

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
