/* eslint-env node */

const path = require("path");
const child_process = require("child_process");
const { DefinePlugin } = require("webpack");

function getDefines() {
    const packageVersion = process.env.npm_package_version;

    const versionChannel = process.env.CFG_RELEASE_CHANNEL || "nightly";

    const buildDate = new Date().toISOString();

    let commitHash = "unknown";
    try {
        commitHash = child_process.execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();
    } catch (_) {
        console.log("Couldn't fetch latest git commit...");
    }

    const versionName = versionChannel === "nightly"
                      ? `nightly ${buildDate.substring(0, 10)}`
                      : packageVersion;

    return {
        VERSION_NUMBER: JSON.stringify(packageVersion),
        VERSION_NAME: JSON.stringify(versionName),
        VERSION_CHANNEL: JSON.stringify(versionChannel),
        BUILD_DATE: JSON.stringify(buildDate),
        COMMIT_HASH: JSON.stringify(commitHash),
    };
}

module.exports = (env, argv) => {
    let mode = "production";
    if (argv && argv.mode) {
        mode = argv.mode;
    }

    console.log(`Building ${mode}...`);

    return {
        mode,
        entry: "./src/index.ts",
        experiments: {
            outputModule: true,
        },
        output: {
            path: path.resolve(__dirname, "dist"),
            filename: "index.js",
            publicPath: "",
            clean: true,
            library: { type: "module" },
        },
        module: {
            rules: [
                {
                    test: /\.ts$/i,
                    use: "ts-loader",
                    exclude: "/node_modules/",
                },
                {
                    test: /\.wasm$/i,
                    type: "asset/resource",
                },
            ],
        },
        resolve: {
            extensions: [".ts", ".js"],
        },
        devtool: "source-map",
        plugins: [
            new DefinePlugin(getDefines()),
        ],
    };
};
