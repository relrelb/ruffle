console.log("background", chrome);
chrome.webNavigation.onBeforeNavigate.addListener(data => {
    console.log("onBeforeNavigate", data);
    chrome.storage.local.set({ lastNavigation: data }, () => {});
});
