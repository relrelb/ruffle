chrome.webNavigation.onBeforeNavigate.addListener((data) => {
    chrome.storage.local.set({ lastNavigation: data });
});
