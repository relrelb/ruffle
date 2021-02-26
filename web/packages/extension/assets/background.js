chrome.webNavigation.onBeforeNavigate.addListener((data) => {
    chrome.storage.local.set({ lastNavigation: data });
});

const MESSAGE_HANDLERS = {
    "fetch": handleFetch,
};

async function handleFetch(port, message) {
    const response = await fetch(message.url);
    const reader = response.body.getReader();
    const total = Number(response.headers.get("content-length"));
    let loaded = 0;
    while (true) {
        const { value, done } = await reader.read();
        if (done) {
            break;
        }
        loaded += value.byteLength;
        console.log("chunk", value, `${loaded} / ${total} = ${loaded / total * 100}%`);
        port.postMessage(Array.from(value));
    }
    port.disconnect();
}

chrome.runtime.onConnect.addListener((port) => {
    // TODO: validate port.sender.
    port.onMessage.addListener((message) => {
        const handler = MESSAGE_HANDLERS[message.type];
        if (!handler) {
            console.warn(`Unhandled message: ${message}`);
            port.disconnect();
            return;
        }
        handler(port, message);
    });
    // port.onDisconnect.addListener(() => {});
});
