import html from "./shadow-template.html";

/**
 * The shadow template which is used to fill the actual Ruffle player element
 * on the page.
 */
export const ruffleShadowTemplate = document.createElement("template");
ruffleShadowTemplate.innerHTML = html;
