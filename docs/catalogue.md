# Socket — catalogue

**Status:** Draft for review
**Date:** 2026-10-08

The first hundred services Socket aims to cover, and the order they are added. The catalogue has no fixed size; this list is where it starts.

## How to read this list

- **The list is a judgement, not a measurement.** No public ranking of the most-integrated services exists. These are the services that appear most consistently across the catalogues of workflow and integration platforms. The one measured signal available, a month of download counts for Activepieces' connectors (September 2026), puts Slack first, then Google Sheets, Google Drive, Gmail, Microsoft Outlook, Salesforce, Notion, HubSpot and Linear among the SaaS entries.
- **Rows are products; providers are vendors.** A vendor is the auth boundary, so Google's seven rows are one provider and Microsoft's eight are another. The hundred rows come from 80 vendors.
- **The auth and API columns are expectations.** They record what each service is generally known to offer and are confirmed against the vendor's documentation when the provider is added. They are not yet verified.
- **Being listed is not being supported.** A service is supported only at the tier its row in the generated README says (design spec, section 10).
- **LLM providers are left out on purpose.** Socket is not an LLM client (design spec, section 2).

## Waves

| Wave | What it is | Services | When |
| --- | --- | --- | --- |
| 1 | The six providers the core is proven against | 11 rows from six vendors | Phase 1 (authorise, identity, generic request); typed operations for GitHub, Slack and Linear in phase 2 |
| 2 | The next most-requested services | 24 rows | Registry entries as soon as the registry format exists; verified integrations as owners appear |
| 3 | The rest of the hundred | 65 rows | Registry entries in phase 5; verified integrations as owners appear |

Wave 2 is also where the core meets auth shapes wave 1 does not have: API keys in a header, basic auth, OAuth client credentials and per-tenant base URLs (Salesforce, Zendesk, Shopify). Amazon SES needs AWS request signing, which is a new auth mode and is deliberately in wave 3.

## The hundred

### Communication

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 1 | Slack | Slack | OAuth 2.0 | REST | 1 |
| 2 | Microsoft Teams | Microsoft | OAuth 2.0 | REST (Graph) | 2 |
| 3 | Discord | Discord | OAuth 2.0 or bot token | REST | 2 |
| 4 | Zoom | Zoom | OAuth 2.0 | REST | 1 |
| 5 | Google Meet | Google | OAuth 2.0 | REST | 1 |
| 6 | Telegram | Telegram | Bot token | REST | 3 |
| 7 | WhatsApp Business | Meta | Access token | REST (Graph) | 3 |
| 8 | Twilio | Twilio | API key (basic) | REST | 2 |
| 9 | Webex | Cisco | OAuth 2.0 | REST | 3 |

### Transactional email

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 10 | Gmail | Google | OAuth 2.0 | REST | 1 |
| 11 | Microsoft Outlook | Microsoft | OAuth 2.0 | REST (Graph) | 2 |
| 12 | SendGrid | Twilio | API key | REST | 2 |
| 13 | Mailgun | Mailgun | API key (basic) | REST | 3 |
| 14 | Postmark | Postmark | API key | REST | 3 |
| 15 | Amazon SES | AWS | AWS request signing | REST | 3 |
| 16 | Resend | Resend | API key | REST | 3 |

### Marketing

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 17 | Mailchimp | Intuit | OAuth 2.0 or API key | REST | 2 |
| 18 | Klaviyo | Klaviyo | OAuth 2.0 or API key | REST | 3 |
| 19 | Brevo | Brevo | API key | REST | 3 |
| 20 | ActiveCampaign | ActiveCampaign | API key | REST | 3 |
| 21 | Customer.io | Customer.io | API key | REST | 3 |
| 22 | Marketo | Adobe | OAuth 2.0 (client credentials) | REST | 3 |

### Calendar and scheduling

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 23 | Google Calendar | Google | OAuth 2.0 | REST | 1 |
| 24 | Outlook Calendar | Microsoft | OAuth 2.0 | REST (Graph) | 2 |
| 25 | Calendly | Calendly | OAuth 2.0 or API key | REST | 2 |
| 26 | Cal.com | Cal.com | OAuth 2.0 or API key | REST | 3 |

### Files, documents and tables

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 27 | Google Drive | Google | OAuth 2.0 | REST | 1 |
| 28 | Google Docs | Google | OAuth 2.0 | REST | 1 |
| 29 | Google Sheets | Google | OAuth 2.0 | REST | 1 |
| 30 | OneDrive | Microsoft | OAuth 2.0 | REST (Graph) | 2 |
| 31 | SharePoint | Microsoft | OAuth 2.0 | REST (Graph) | 2 |
| 32 | Dropbox | Dropbox | OAuth 2.0 | REST (RPC style) | 2 |
| 33 | Box | Box | OAuth 2.0 | REST | 3 |
| 34 | Notion | Notion | OAuth 2.0 | REST | 1 |
| 35 | Confluence | Atlassian | OAuth 2.0 | REST | 2 |
| 36 | Airtable | Airtable | OAuth 2.0 or API key | REST | 2 |
| 37 | Coda | Coda | API key | REST | 3 |
| 38 | Smartsheet | Smartsheet | OAuth 2.0 or API key | REST | 3 |

### Project and task management

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 39 | Jira | Atlassian | OAuth 2.0 | REST | 2 |
| 40 | Linear | Linear | OAuth 2.0 | GraphQL | 1 |
| 41 | Asana | Asana | OAuth 2.0 | REST | 2 |
| 42 | Trello | Atlassian | API key and token | REST | 2 |
| 43 | ClickUp | ClickUp | OAuth 2.0 or API key | REST | 3 |
| 44 | monday.com | monday.com | OAuth 2.0 or API key | GraphQL | 3 |
| 45 | Basecamp | 37signals | OAuth 2.0 | REST | 3 |
| 46 | Wrike | Wrike | OAuth 2.0 | REST | 3 |
| 47 | Todoist | Doist | OAuth 2.0 or API key | REST | 3 |

### Developer tools and operations

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 48 | GitHub | GitHub | OAuth 2.0, app or token | REST and GraphQL | 1 |
| 49 | GitLab | GitLab | OAuth 2.0 or API key | REST and GraphQL | 2 |
| 50 | Bitbucket | Atlassian | OAuth 2.0 | REST | 3 |
| 51 | Azure DevOps | Microsoft | OAuth 2.0 or API key | REST | 3 |
| 52 | Sentry | Sentry | OAuth 2.0 or API key | REST | 3 |
| 53 | PagerDuty | PagerDuty | OAuth 2.0 or API key | REST | 3 |
| 54 | Datadog | Datadog | API key | REST | 3 |
| 55 | Vercel | Vercel | OAuth 2.0 or API key | REST | 3 |
| 56 | CircleCI | CircleCI | API key | REST | 3 |
| 57 | Opsgenie | Atlassian | API key | REST | 3 |

### CRM and sales

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 58 | Salesforce | Salesforce | OAuth 2.0 | REST and SOQL | 2 |
| 59 | HubSpot | HubSpot | OAuth 2.0 or API key | REST | 2 |
| 60 | Pipedrive | Pipedrive | OAuth 2.0 or API key | REST | 3 |
| 61 | Zoho CRM | Zoho | OAuth 2.0 | REST | 3 |
| 62 | Dynamics 365 | Microsoft | OAuth 2.0 | REST (OData) | 3 |
| 63 | Close | Close | OAuth 2.0 or API key | REST | 3 |
| 64 | Attio | Attio | OAuth 2.0 or API key | REST | 3 |
| 65 | Apollo | Apollo | API key | REST | 3 |
| 66 | Outreach | Outreach | OAuth 2.0 | REST (JSON:API) | 3 |
| 67 | Gong | Gong | OAuth 2.0 or API key | REST | 3 |

### Customer support

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 68 | Zendesk | Zendesk | OAuth 2.0 or API key | REST | 2 |
| 69 | Intercom | Intercom | OAuth 2.0 | REST | 2 |
| 70 | Freshdesk | Freshworks | API key (basic) | REST | 3 |
| 71 | Help Scout | Help Scout | OAuth 2.0 | REST | 3 |
| 72 | Front | Front | OAuth 2.0 or API key | REST | 3 |
| 73 | ServiceNow | ServiceNow | OAuth 2.0 | REST | 3 |

### Payments and finance

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 74 | Stripe | Stripe | OAuth 2.0 or API key | REST | 2 |
| 75 | PayPal | PayPal | OAuth 2.0 (client credentials) | REST | 3 |
| 76 | Square | Block | OAuth 2.0 | REST | 3 |
| 77 | QuickBooks Online | Intuit | OAuth 2.0 | REST | 3 |
| 78 | Xero | Xero | OAuth 2.0 | REST | 3 |
| 79 | Chargebee | Chargebee | API key (basic) | REST | 3 |
| 80 | Paddle | Paddle | API key | REST | 3 |
| 81 | Plaid | Plaid | Client id and secret | REST | 3 |

### Commerce

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 82 | Shopify | Shopify | OAuth 2.0 | GraphQL and REST | 2 |
| 83 | WooCommerce | Automattic | Consumer key and secret | REST | 3 |
| 84 | BigCommerce | BigCommerce | OAuth 2.0 | REST | 3 |

### People and hiring

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 85 | Workday | Workday | OAuth 2.0 | REST and SOAP | 3 |
| 86 | BambooHR | BambooHR | OAuth 2.0 or API key | REST | 3 |
| 87 | Gusto | Gusto | OAuth 2.0 | REST | 3 |
| 88 | Rippling | Rippling | OAuth 2.0 or API key | REST | 3 |
| 89 | Greenhouse | Greenhouse | API key (basic) | REST | 3 |
| 90 | Lever | Lever | OAuth 2.0 or API key | REST | 3 |

### Identity

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 91 | Okta | Okta | OAuth 2.0 or API key | REST | 3 |
| 92 | Microsoft Entra ID | Microsoft | OAuth 2.0 | REST (Graph) | 3 |

### Analytics

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 93 | Google Analytics | Google | OAuth 2.0 | REST | 3 |
| 94 | Mixpanel | Mixpanel | Service account (basic) | REST | 3 |
| 95 | Amplitude | Amplitude | API key | REST | 3 |
| 96 | Segment | Twilio | API key | REST | 3 |

### Forms, signing and design

| # | Service | Vendor | Auth (expected) | API style | Wave |
| --- | --- | --- | --- | --- | --- |
| 97 | Typeform | Typeform | OAuth 2.0 or API key | REST | 3 |
| 98 | DocuSign | DocuSign | OAuth 2.0 | REST | 3 |
| 99 | Figma | Figma | OAuth 2.0 or API key | REST | 2 |
| 100 | Miro | Miro | OAuth 2.0 | REST | 3 |

## Next after the hundred

Advertising and social APIs (Google Ads, Meta, LinkedIn, X, TikTok, YouTube), PostHog, Snowflake and BigQuery, Webflow, WordPress, Contentful, Loom, Canva, Jotform, PandaDoc, Recurly, Deel, Auth0, Salesloft, Jira Service Management, Statuspage. These wait for the registry and for someone who needs them.
