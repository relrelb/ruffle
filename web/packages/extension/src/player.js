import * as utils from "./utils";

window.addEventListener("DOMContentLoaded", async () => {
    const url = new URL(window.location);
    const swfUrl = url.searchParams.get("url");
    if (!swfUrl) {
        const { lastNavigation } = await utils.storage.local.get(
            "lastNavigation"
        );
        if (!lastNavigation) {
            return;
        }
        utils.storage.local.remove("lastNavigation");
        url.search = `?url=${lastNavigation.url}`;
        document.location = url;
        return;
    }

    const iframe = document.getElementById("sandbox");
    const iframeLoaded = new Promise((resolve) => iframe.addEventListener("load", () => resolve()));

    const swfData = await (await fetch(swfUrl)).arrayBuffer();
    await iframeLoaded;
    iframe.contentWindow.postMessage(swfData, "*", [swfData]);
});
