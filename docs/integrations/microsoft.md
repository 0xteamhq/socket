# Microsoft

**Status:** built and tested against a local server that answers as Microsoft's documentation says. Not yet run against the real Microsoft Graph.

Socket's Microsoft integration is one provider for everything behind Microsoft Graph: Outlook, Teams, OneDrive, SharePoint and Entra ID share one sign-in. This page shows how to connect, lists what is supported, and says what was and was not confirmed against Microsoft's documentation.

## Connect

Add the crate with the Microsoft feature:

```sh
cargo add socketkit --features microsoft
```

### With a token you already hold

```rust
use std::sync::Arc;
use socketkit::microsoft::Microsoft;
use socketkit::{ConnectionKey, ProviderId, Socket};

let microsoft = Microsoft::with_token("eyJ0eXAi…");
let socket = Socket::in_memory().integration(Arc::new(microsoft.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("microsoft")?, "me")).await?;
```

Every call uses that token. A Graph access token lasts about an hour and this way has no refresh token, so it suits a script, not a connection that has to keep working.

### With your own app registration, to connect your users

Register an application in the Microsoft Entra admin centre, add a client secret, and add your callback route as a **Web** redirect URI.

```rust
use socketkit::microsoft::{Microsoft, MicrosoftOAuth, Prompt};
use socketkit::{OAuthClient, SecretString};

let microsoft = Microsoft::with_oauth(MicrosoftOAuth {
    client: OAuthClient {
        client_id: config.microsoft_client_id,
        client_secret: SecretString::new(config.microsoft_client_secret),
        redirect_uri: "https://yourapp.example/oauth/microsoft/callback".parse()?,
    },
    // Graph permissions. `None` asks for the defaults: offline_access and User.Read.
    scopes: Some(vec!["User.Read".into(), "Calendars.Read".into()]),
    // Who may sign in. `None` is `common`.
    tenant: Some("organizations".into()),
    login_hint: None,
    prompt: Some(Prompt::SelectAccount),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(microsoft.clone())).build()?;

// When a user clicks "Connect Microsoft":
let key = ConnectionKey::new(ProviderId::new("microsoft")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`Microsoft::with_oauth(client)` with a plain `OAuthClient` also works when the defaults are enough.

| Setting | What it does |
| --- | --- |
| `scopes` | The Graph permissions to ask for, in place of the defaults. Every product needs its own: a calendar needs `Calendars.Read`, and so on. |
| `tenant` | `common` (any account, the default), `organizations` (work or school accounts), `consumers` (personal accounts), or a tenant id or domain for one organisation. A guest signing in to another organisation needs that organisation's id or domain. |
| `login_hint` | Prefills the sign-in page with an email address. |
| `prompt` | `Login`, `None`, `Consent` or `SelectAccount`: what the sign-in page asks of the person. |

Three things Socket does for every Microsoft connection:

- **A refresh token is always asked for.** `offline_access` is added to whatever scopes are given, in the settings or at `begin_authorization`. Without it Microsoft issues no refresh token and the connection stops working within the hour.
- **Each refresh saves the new refresh token.** Microsoft sends a new one every time and expects the old one to be discarded.
- **PKCE is on.** The code challenge is sent with the sign-in and the verifier with the code, beside the client secret.

A tenant that is not one of the keywords, an id or a domain is reported when the `Socket` is built, because the tenant is part of the address of the sign-in page.

### When a permission needs an administrator

Many Graph permissions, most of those for Teams among them, can only be approved by an administrator of the organisation. When Microsoft's token endpoint answers that consent is missing, Socket reports `AccessDenied`, not `ReconnectRequired`, with a message that says who has to approve:

- `AADSTS65001` or `AADSTS90008`, or the error `consent_required`: the person has to accept the permission at sign-in, or an administrator has to approve it.
- `AADSTS90094` or `AADSTS90095`: an administrator has to approve it.

The stored tokens are kept, since they work again once the permission is approved. Any other refused refresh, such as a refresh token that expired after 90 days unused or was revoked by a password change, is `ReconnectRequired`.

Most consent refusals never reach the token endpoint: Microsoft sends the person back to your callback with `error=access_denied` or `error=consent_required` and no code. That redirect is your application's to handle.

## Identity and lookup

```rust
let me = microsoft.identity(&connection).await?;        // Account { id, name, email }
let item = microsoft.resolve(&connection, link).await?; // Resource { id, label, description }
```

`identity` reads `GET /me` and needs `User.Read`. The email is the mailbox address, or the sign-in name (`userPrincipalName`) for an account without a mailbox.

`resolve` accepts a OneDrive or SharePoint sharing link, the kind the Share button copies, and confirms it leads to a file or folder the account can open. The resource's `id` is the Graph path that addresses the item, `drives/{drive id}/items/{item id}`, so it can be passed to a later request; the label is the item's name. It needs `Files.ReadWrite`: Microsoft lists no read-only permission for opening a sharing link.

## Call an operation by name

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `microsoft.identity.get` | read |  | Return the account this connection is authorised as, confirming the token still works. |
| `microsoft.resource.resolve` | read |  | Confirm that a resource exists and the account can reach it. Accepts a OneDrive or SharePoint sharing link. |

## Handle errors

| Kind | What it means for Microsoft | What to do |
| --- | --- | --- |
| `ReconnectRequired` | Graph answered 401 or called the token invalid, or the refresh token is no longer accepted | Socket renews the token once by itself; if that fails, connect again |
| `AccessDenied` | A permission is missing or not yet approved, or the account may not open the item. The message carries Graph's own reason | Add the permission, or have an administrator approve it |
| `NotFound` | No such item, or a sharing link that no longer exists | Check the id or the link |
| `InvalidInput` | Graph refused the request; the message carries its reason | Fix the input |
| `RateLimited` | Graph is throttling; `retry()` says how long to wait | Wait and try again |
| `Unexpected` | Graph failed. On a 503 that states a wait, `retry()` carries it | Try again later |

## Confirmed against Microsoft's documentation, and not

Everything here was read from learn.microsoft.com in October 2026. Nothing was run against a live tenant.

Confirmed:

- The authorise and token addresses, the four kinds of tenant, space-separated scopes, and that `offline_access` is what yields a refresh token.
- PKCE with `S256` is recommended for web applications and shown beside the client secret.
- A refresh returns a new refresh token, which replaces the old one.
- `login_hint`, and the four `prompt` values.
- The client secret is sent in the form body.
- The numbered codes `AADSTS65001`, `90008`, `90094` and `90095`, with the meanings above.
- Graph's error shape `{ "error": { "code", "message" } }`, and `Retry-After` in seconds on 429 and on 503.
- `GET /me` and its fields, with `User.Read`.
- `GET /shares/{encoded link}/driveItem`, the encoding (`u!`, then the link in base64url without padding), and `Files.ReadWrite` as its least permission.

Not confirmed:

- **`InvalidAuthenticationToken`** is not in Microsoft's error reference; it is treated as "renew the token" because the issue that asked for this provider says Graph returns it. A 401 is handled the same way with or without that code.
- **Which `error` value accompanies each numbered code** at the token endpoint. Socket goes by the number, and by `consent_required` when there is no number. Microsoft warns that the numbers may change over time.
- **What Graph answers when a sharing link cannot be opened.** Socket treats a 403 as a refusal and a 404 as not found, and cannot tell a missing permission from an item the account may not see.
- **Whether a sharing link's item always carries `parentReference.driveId`.** When it does not, the resource's id is `shares/{encoded link}/driveItem`, which addresses the same item.

## Not supported yet

- **Application-only access** (client credentials), where no person signs in.
- **The national clouds** (US Government, China), which use other hosts.
- **Certificates** in place of a client secret.
- **The products themselves**: mail, calendar, Teams, files. Each is its own piece of work on top of this provider.

Anything Graph offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("me/messages").with_query("$top", "5")).await?;
```
