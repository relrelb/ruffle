import { PublicAPI, SourceAPI, publicPath } from "ruffle-core";

window.RufflePlayer = PublicAPI.negotiate(
    window.RufflePlayer,
    "local",
    new SourceAPI("local")
);
__webpack_public_path__ = publicPath(window.RufflePlayer.config, "local");

let ruffle;
let player;

// Default config used by the player.
const config = {
    letterbox: "on",
    logLevel: "warn",
};

window.addEventListener("DOMContentLoaded", () => {
    ruffle = window.RufflePlayer.newest();
    player = ruffle.createPlayer();
    player.id = "player";
    document.getElementById("main").append(player);
});

window.addEventListener("message", (event) => {
    player.load({ data: event.data, ...config });
});
