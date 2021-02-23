import {
    getI18nMessage,
    getSyncStorage,
    setSyncStorage,
    addStorageChangeListener,
} from "./utils";

function camelize(string) {
    return string
        .toLowerCase()
        .replace(/[^a-zA-Z0-9]+(.)/g, (_, char) => char.toUpperCase());
}

function getBooleanElements() {
    const elements = {};
    for (const option of document.getElementsByClassName("option")) {
        const [checkbox] = option.getElementsByTagName("input");
        if (checkbox.type !== "checkbox") {
            continue;
        }
        const [label] = option.getElementsByTagName("label");
        const key = camelize(checkbox.id);
        elements[key] = { option, checkbox, label };
    }
    return elements;
}

export async function bindBooleanOptions() {
    const elements = getBooleanElements();

    // Bind initial values.
    const options = await getSyncStorage(Object.keys(elements));
    for (const [key, value] of Object.entries(options)) {
        elements[key].checkbox.checked = value;
    }

    for (const [key, { checkbox, label }] of Object.entries(elements)) {
        // TODO: click/change/input?
        checkbox.addEventListener("click", () => {
            const value = checkbox.checked;
            options[key] = value;
            setSyncStorage({ [key]: value });
        });

        label.textContent = getI18nMessage(`settings_${checkbox.id}`);

        // Prevent transition on load.
        // Method from https://stackoverflow.com/questions/11131875.
        label.classList.add("notransition");
        label.offsetHeight; // Trigger a reflow, flushing the CSS changes.
        label.classList.remove("notransition");
    }

    // Listen for future changes.
    addStorageChangeListener((changes) => {
        for (const [key, option] of Object.entries(changes)) {
            elements[key].checkbox.checked = option.newValue;
            options[key] = option.newValue;
        }
    });
}
