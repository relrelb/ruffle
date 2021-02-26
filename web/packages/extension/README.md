# ruffle-extension

ruffle-extension is all of the power of Ruffle, in your browser.

Without needing websites to do anything, the browser extension automatically replaces any Flash content on websites with the Ruffle player.

It automatically negotiates with websites that do have Ruffle installed, to ensure that there is no conflict between the versions. Newer version of ruffle, either from the website or extension, will always take precedence and disable the other.

## Using ruffle-extension

The browser extension is built to work in both Chrome and Firefox.

We do not yet have a signed release of the extension, so you must load it as a temporary extension.

Before you can install the extension, you must either download the [latest release](https://github.com/ruffle-rs/ruffle/releases) or [build it yourself](../../README.md).

### Chrome

- Navigate to `chrome://extensions/`.
- Turn on Developer mode in the top-right corner.
- Drag and drop `ruffle_extension.zip` into the page.

Alternatively, loading unpacked can save time during development:

- Navigate to `chrome://extensions/`.
- Turn on Developer mode in the top-right corner.
- Click "Load unpacked".
- Select the `assets/` folder.
- Each time after making changes, click the reload icon.

### Firefox

- Navigate to `about:debugging`.
- Click on "This Firefox".
- Click "Load Temporary Add-on...".
- Select the `.xpi` from the `dist/` folder.

## Building, testing or contributing

Please see the [ruffle-web README](../../README.md).
