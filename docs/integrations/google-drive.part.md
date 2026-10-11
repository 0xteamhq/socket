### Drive: `drive_files` and `drive_shared_drives`

Find files and folders, read what describes one, read a Google document as text, see who can open a file, and file things: make a folder, copy, move, rename, and put in the bin. A Google Doc, a Sheet, a folder and a shortcut are all files in Drive; `mimeType` says which.

```rust
use socketkit::google::models::{DriveCreateFolder, DriveExportFormat, DriveListFiles, Paging};

let drive = google.drive_files(&connection);

// The Docs changed this month that mention the plan, newest first.
let found = drive.list(DriveListFiles {
    q: Some(
        "fullText contains 'quarterly plan' and mimeType = 'application/vnd.google-apps.document' \
         and modifiedTime > '2026-10-01T00:00:00' and trashed = false".into(),
    ),
    ..Default::default()
}).await?;

// Before showing a document to other people, see who was allowed to read it.
let doc = &found.items[0];
let who = drive.permissions(&doc.id, Paging::default()).await?;
let text = drive.export(&doc.id, DriveExportFormat::Markdown).await?.text;

// File it away.
let archive = drive.create_folder(DriveCreateFolder {
    name: Some("Archive 2026".into()),
    ..Default::default()
}).await?;
drive.move_to(&doc.id, &archive.id).await?;
```

| Group | Method | What it does | Effect | Scope |
| --- | --- | --- | --- | --- |
| `drive_files` | `list(DriveListFiles)` | `Page<DriveFile>`: what matches a search, or everything the account can see | read | `drive.readonly` |
| `drive_files` | `get(file)` | `DriveFile`: what describes one file or folder, not its content | read | `drive.readonly` |
| `drive_files` | `export(file, DriveExportFormat)` | `DriveExport`: a Google document as text | read | `drive.readonly` |
| `drive_files` | `permissions(file, Paging)` | `Page<DrivePermission>`: who can see a file, and in what role | read | `drive.readonly` |
| `drive_files` | `create_folder(DriveCreateFolder)` | `DriveFile`: the new folder | write | `drive.file` |
| `drive_files` | `copy(file, DriveCopyFile)` | `DriveFile`: the copy | write | `drive.file` |
| `drive_files` | `move_to(file, folder)` | `DriveFile`, in its new folder | write | `drive.file` |
| `drive_files` | `rename(file, name)` | `DriveFile`, under its new name | write | `drive.file` |
| `drive_files` | `trash(file)` | `DriveFile`, in the bin | destructive | `drive.file` |
| `drive_shared_drives` | `list(Paging)` | `Page<SharedDrive>`: the shared drives the account is a member of | read | `drive.readonly` |

`drive.readonly` is one of the provider's default scopes. `drive.file` is not: name it in `GoogleOAuth::scopes` to use the writes. It is the narrowest scope Google offers for them, and it reaches only the files the application created or the person opened with it, through Google's file picker for one. A write to any other file is refused with 403, which arrives as `AccessDenied` with Google's words ("The user has not granted the app … access to the file …"). To change every file the account can, ask for the full `https://www.googleapis.com/auth/drive` scope instead; the operations work with it unchanged.

**An id, or a link.** Every `file` and `folder` is a Drive id. `google.resource.resolve` turns a link someone pasted (`https://docs.google.com/document/d/…/edit`, `https://drive.google.com/drive/folders/…`) into the id, and confirms the account can open it. Where Google takes a folder's id it also takes `root`, the top of the account's own My Drive: `get("root")`, `'root' in parents`, `move_to(file, "root")`.

**What a `DriveFile` carries:** `id`, `name`, `mimeType`, `parents`, `createdTime`, `modifiedTime`, `size`, `owners` (each with `displayName`, `emailAddress`, `permissionId`, `me`), `webViewLink`, `trashed`, `driveId` and `shortcutDetails` (`targetId`, `targetMimeType`, `targetResourceKey`). Drive returns only the fields a request names, so every request names exactly these and no other field of a file can come back. A test compares what is asked for with what the types hold, for each operation.

- `mimeType` is `application/vnd.google-apps.folder` for a folder, `…document` for a Doc, `…spreadsheet` for a Sheet, `…presentation` for Slides and `…shortcut` for a shortcut. Anything else is a file with content of its own, such as `application/pdf`.
- `parents` holds the one folder a file is in. It is empty when the account cannot see that folder, as with a file someone shared from their own Drive.
- `size` is a number in a string, as Google writes it. A folder and a shortcut have none.
- `owners` is empty for a file in a shared drive, which belongs to the drive; `driveId` names the drive.
- A shortcut is a file of its own that points at another. Read the other by `shortcutDetails.targetId`.

**Shared drives are always in reach.** Every request that names a file or lists files says `supportsAllDrives=true`, and `list` also says `includeItemsFromAllDrives=true`. Without them Google answers as if what is in a shared drive did not exist, with no error. There is no option to turn this off.

**Listing.** `DriveListFiles` has `q`, `orderBy`, `driveId`, `limit` and `cursor`. With nothing set, the list is of everything the account can see, what is in the bin included, in no particular order.

- `q` is Drive's own query language and goes to Google exactly as you wrote it. Socket does not read it, so a mistake in it is Google's to report: a 400, which arrives as `InvalidInput` with Google's words.
- `orderBy` is Google's sort keys with commas between them, each followed by ` desc` to reverse it: `folder,modifiedTime desc`. The keys are `createdTime`, `folder`, `modifiedByMeTime`, `modifiedTime`, `name`, `name_natural`, `quotaBytesUsed`, `recency`, `sharedWithMeTime`, `starred` and `viewedByMeTime`. Google refuses a sort together with a `fullText` search, whose results are always ordered by relevance.
- `driveId` lists what is in one shared drive. Socket sends `corpora=drive` with it, which is what makes Google search that drive. Get the id from `drive_shared_drives.list`.
- A page holds at most 1000 files (`limit`), and Google may return fewer than asked for. Pass `next_cursor` back as `cursor` with the same `q` and `orderBy`.

Common searches:

| To find | `q` |
| --- | --- |
| A file by its exact name | `name = 'Q4 plan'` |
| Names that contain a word | `name contains 'budget'` |
| Words anywhere in the content | `fullText contains 'quarterly plan'` |
| An exact phrase in the content | `fullText contains '"quarterly plan"'` |
| One type of file | `mimeType = 'application/vnd.google-apps.spreadsheet'` |
| Everything but folders | `mimeType != 'application/vnd.google-apps.folder'` |
| What is in a folder | `'FOLDER_ID' in parents` |
| Changed since a time | `modifiedTime > '2026-10-01T00:00:00'` |
| Not in the bin | `trashed = false` |
| Shared with the account | `sharedWithMe` |
| Owned by someone | `'ada@example.test' in owners` |

Join them with `and`, `or` and `not`: `'FOLDER_ID' in parents and trashed = false`. A time is RFC 3339 and is read as UTC unless it carries an offset. `contains` on a `name` matches a prefix only: `name contains 'Hello'` finds "HelloWorld" and `name contains 'World'` does not. On `fullText` it matches whole words.

**Quoting a value.** A value goes in single quotes. Inside it, write a single quote as `\'` and a backslash as `\\`: a file named `quinn's paper\essay` is found with `name contains 'quinn\'s paper\\essay'`. When the value comes from a person, do that replacement before putting it in `q`, backslashes first: Socket cannot do it for you, because it does not read the query.

**Exporting a Google document.** `export` returns a Doc, a Sheet or a Slides presentation as text, in `text`. `DriveExportFormat` is the whole choice:

| Format | In JSON | For |
| --- | --- | --- |
| `Text` | `text/plain` | A Doc, or a Slides presentation |
| `Markdown` | `text/markdown` | A Doc, with its headings, lists, links and tables |
| `Csv` | `text/csv` | A Sheet. **Only its first sheet is exported.** For the others, read the spreadsheet through Sheets |

- Any other format is refused before Google is called. PDF, Word, Excel and the rest are bytes, which Socket's transport does not carry yet.
- **An export is at most 10 MB.** Google refuses a larger one, and Socket itself reads an answer of at most 10 MB. Either way the call fails with `InvalidInput` and the message "this file is too large to export: the limit is 10 MB of exported content". There is no way around it here; a document that large has to be read in parts through Docs or Sheets.
- **Only a Google document can be exported.** A PDF, an image or a Word file has content of its own and nothing to export. Google refuses it, and the call fails with `InvalidInput` and "this file is not a Google document, so there is nothing to export". Check `mimeType` first: it starts with `application/vnd.google-apps.` for the files that can be.
- A format that does not suit the file, such as `Csv` for a Doc, is Google's to refuse: `InvalidInput` with Google's words.
- An empty document is an empty `text`, not an error. The byte order mark Google puts before a Doc's plain text is left out; line endings are as Google wrote them.

**Who can see a file.** `permissions` lists every grant on a file or a folder. Each `DrivePermission` has `id`, `type` (`user`, `group`, `domain` or `anyone`), `role` (`owner`, `organizer`, `fileOrganizer`, `writer`, `commenter` or `reader`), `emailAddress` for a person or a group, `domain` for a domain, `displayName`, `deleted` (the account it was granted to no longer exists), `allowFileDiscovery`, `expirationTime` and `permissionDetails`.

- `type: "anyone"` with `allowFileDiscovery: false` is "anyone with the link". With `true`, the file can also be found by searching.
- `permissionDetails` says where each grant comes from: `permissionType` is `file` for a grant on a file or folder and `member` for membership of a shared drive, `inherited` says whether it comes from above, and `inheritedFrom` names the folder or drive it comes from.
- A page holds at most 100 permissions. An account that may not see who a file is shared with is refused by Google, which arrives as `AccessDenied`.

**Making a folder.** `DriveCreateFolder` has `name`, which is required, and `parents`: a list of the one folder to create it in. Without `parents` the folder is made at the top of the account's My Drive. To make one in a shared drive, give the drive's id or a folder inside it. Drive allows two folders of the same name in one place, so calling this twice makes two.

**Copying.** `DriveCopyFile` has `name` and `parents`, both optional: without them Google names the copy "Copy of …" and puts it beside the original. Google does not copy a folder. A copy is a new file with a new id.

**`parents` is a list of exactly one.** That is how Google writes it, and a file has one parent. A list of none or of several is refused before Google is called.

**Moving.** `move_to(file, folder)` puts a file or a folder into another folder and takes it out of the one it was in. It keeps its id. Google moves a file by being told which parent to add and which to remove, so Socket reads the file first and then sends the change: two requests.

- **Already there.** A file that is in the folder, and nowhere else, is returned as it is. No change is sent.
- **A file whose folder the account cannot see**, such as one shared from someone else's Drive, names no parent. Socket then only adds the new one. Google moves the file if the account may, and refuses with 403 if it may not; nothing is changed by the refusal.
- **A file in several folders**, which only files from before 2020 can be, ends up in the one folder it was moved to.
- **`root`** is looked up first, with one more read, because a file names its parent by id and never as `root`.
- A file is not moved into itself; that is refused before Google is called.
- If someone else moves the file between the read and the change, Google is asked to remove a parent the file no longer has. What Google does then was not confirmed; read the file again and repeat the move.
- Moving into or between shared drives has rules of its own (who may move, and that a folder cannot always follow). Google's refusal arrives as `AccessDenied` with its reason.

**Renaming.** `rename(file, name)` changes the name and nothing else. A blank name is refused.

**The bin.** `trash` puts a file in the bin, and a folder with everything in it. Nothing is deleted: it can be taken out again for 30 days, after which Google empties it. Only a file's owner can bin it, or in a shared drive someone whose role allows it; anyone else is refused with `AccessDenied`. Taking a file out of the bin, and deleting one for good, are not offered.

**A change is sent once.** A POST or a PATCH that fails with a server error may have been made, so Socket does not send it again. After such a failure on `create_folder` or `copy`, list the folder before trying again, or there may be two.

**Shared drives.** `drive_shared_drives.list` returns each drive's `id`, `name`, `createdTime` and `hidden`. The `id` is what `DriveListFiles.driveId` takes, and is also the id of the drive's top folder, so it can be given as a parent. A page holds 10 drives unless `limit` says otherwise, up to 100.

#### Confirmed against Google's documentation, and not

Everything here was read from developers.google.com in October 2026. Nothing was run against a live account.

Confirmed:

- `GET drive/v3/files` with `q`, `orderBy` and its eleven keys, `corpora`, `driveId`, `includeItemsFromAllDrives`, `supportsAllDrives`, `pageSize` (at most 1000; larger values are coerced) and `pageToken`; the answer's `kind: "drive#fileList"`, `files` and `nextPageToken`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list>
- That `corpora=drive` needs `driveId`, and that shared drive items are left out without `includeItemsFromAllDrives` and `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/guides/enable-shareddrives>
- `GET drive/v3/files/{fileId}` with `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/get>
- The fields of a file with their spelling and types, that `size` is a string, that a file has one parent, and the fields of a user. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/User>
- The `fields` parameter: commas, brackets for the fields of a list or an object, and that `files.list` returns only `kind`, `id`, `name` and `mimeType` without it. <https://developers.google.com/workspace/drive/api/guides/fields-parameter>
- The query language: the terms and their operators, single quotes, `\'` and `\\`, double quotes for a phrase, that `contains` matches a prefix of a name and whole words in content, RFC 3339 times in UTC, and every search in the table above. <https://developers.google.com/workspace/drive/api/guides/search-files>, <https://developers.google.com/workspace/drive/api/guides/ref-search-terms>
- That a sort with a `fullText` search is refused with 400 `badRequest`. <https://developers.google.com/workspace/drive/api/guides/handle-errors>
- `GET drive/v3/files/{fileId}/export` with `mimeType` as its only parameter, and that exported content is limited to 10 MB. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/export>
- The export formats: `text/plain` and `text/markdown` for a Doc, `text/plain` for Slides, `text/csv` for a Sheet with "first sheet only". <https://developers.google.com/workspace/drive/api/guides/ref-export-formats>
- `GET drive/v3/files/{fileId}/permissions` with `supportsAllDrives`, `pageSize` (at most 100) and `pageToken`; `kind: "drive#permissionList"`; the fields of a permission and of `permissionDetails`, and the values of `type`, `role` and `permissionType`. <https://developers.google.com/workspace/drive/api/reference/rest/v3/permissions/list>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/permissions>
- `POST drive/v3/files` for a file with no content, a folder's MIME type, and that a file without `parents` goes to the top of My Drive. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/create>, <https://developers.google.com/workspace/drive/api/guides/folder>
- `POST drive/v3/files/{fileId}/copy` with a file as its body. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/copy>
- `PATCH drive/v3/files/{fileId}` with `addParents`, `removeParents` and `supportsAllDrives`, that only the fields sent are changed, and that a move reads the file's parents first. <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/update>, <https://developers.google.com/workspace/drive/api/guides/folder>
- That a file is binned by setting `trashed` to `true`, that the bin is emptied after 30 days, that only the owner can bin a file, and that a shared drive file needs `supportsAllDrives`. <https://developers.google.com/workspace/drive/api/guides/delete>
- `GET drive/v3/drives` with `pageSize` (10 by default, at most 100) and `pageToken`; `kind: "drive#driveList"`; the fields of a shared drive. <https://developers.google.com/workspace/drive/api/reference/rest/v3/drives/list>, <https://developers.google.com/workspace/drive/api/reference/rest/v3/drives>
- The scopes each method accepts: `drive.readonly` for every read here, `drive.file` for create, copy and update. The reads of files also accept `drive.file`, for the files that scope reaches; the list of shared drives does not.
- The errors `insufficientFilePermissions`, `appNotAuthorizedToFile`, `fileNotExportable` and the shared drive refusals, all 403. <https://developers.google.com/workspace/drive/api/guides/handle-errors>

Not confirmed:

- **How Google reports an export that is too large.** Its documentation states the 10 MB limit and not the error. Reports from people who met it give a 403 with the reason `exportSizeLimitExceeded` and the message "This file is too large to be exported.", and that is what Socket recognises.
- **How Google reports an export of a file that is not a Google document.** The documentation lists the reason `fileNotExportable` only with a message about Google Vids. The message Socket recognises, "Export only supports Docs Editors files.", is from experience of the API and not from a page.
- **That these two are told by their message.** Socket's error for a 403 carries Google's message and not the `reason` beside it, so the two cases are recognised by their wording. If Google rewords either, the call still fails, as `AccessDenied` with Google's own words, and not with the clearer message.
- **That a Doc's plain text begins with a byte order mark.** It is what the API returns; no page says so. Socket removes one if it is there.
- **`fields` on `drives.list` and on the writes.** It is a parameter of every Google API, and the pages for these methods do not list it separately.
- **A PATCH with an empty object as its body**, which is what a move sends. Google's example sends no body at all. Socket's transport gives no length to a request without a body, which a server may refuse, and an empty object changes nothing.
- **What `files.list` returns by default for shared drives.** With no `driveId`, Google searches its `user` body of files with shared drive items included. Exactly which shared drive files that covers is not stated; to be sure of all of one drive, give its `driveId`.
- **That `export` reaches a file in a shared drive.** The method has no `supportsAllDrives` parameter, so none is sent.
- **Removing a parent the file no longer has**, and **adding a parent to a file whose own parent cannot be seen.** See "Moving".
- **That Google refuses to copy a folder**, and with what error. The page for `copy` does not mention folders.
- **Who may bin a file in a shared drive.** The table of roles did not load.
- **Which files `drive.file` reaches.** What is said above is Google's description of the scope as it is generally given; its page was not reread.
- **`root` as a value of `addParents`.** Socket does not rely on it: it looks up the id and sends that.

#### Not supported yet

- **`drive_files.download`**: the content of a file that is not a Google document, such as a PDF. It is bytes, which Socket's transport does not carry yet. It follows issue #6.
- **Exporting to a format that is not text** (PDF, Word, Excel, PowerPoint, images), and **an export over 10 MB**. Both follow issue #6.
- **Other sheets of a spreadsheet as CSV.** Drive exports the first only; the rest are read through Sheets.
- **Searching every shared drive at once, or a whole domain** (`corpora` of `allDrives` or `domain`). Google may then search only part of what was asked and say so in `incompleteSearch`, which a page of results has no place for.
- **Searching for shared drives** (`q` on `drive_shared_drives.list`), and the lists an administrator sees (`useDomainAdminAccess`).
- **Permanent deletion**, left out on purpose, and **taking a file out of the bin**.
- **Sharing**: adding, changing or removing a permission.
- **Uploading a file, or changing a file's content.** `create_folder` and `copy` are the only ways to make a file here.
- **Creating a shortcut**, and following one: read the target by `shortcutDetails.targetId`.
- **Other fields of a file**, such as `description`, `starred`, `lastModifyingUser`, `capabilities`, `exportLinks` and labels, and **changing anything but a file's name, folder and bin**.
- **Comments, revisions, change tracking and notifications.**
