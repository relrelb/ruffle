/* eslint-env node */

const path = require("path");
const CopyWebpackPlugin = require("copy-webpack-plugin");

module.exports = (env, argv) => {
    let mode = "production";
    if (argv && argv.mode) {
        mode = argv.mode;
    }

    console.log(`Building ${mode}...`);

    return {
        mode,
        entry: {
            ruffle: path.resolve(__dirname, "src/index.js"),
            main: path.resolve(__dirname, "src/main.js"),
            options: path.resolve(__dirname, "src/options.js"),
            lv0: path.resolve(__dirname, "src/lv0.js"),
        },
        output: {
            path: path.resolve(__dirname, "assets/dist"),
            filename: "[name].js",
            publicPath: "",
            chunkFilename: "core.ruffle.js",
            clean: true,
        },
        module: {
            rules: [
                {
                    test: /\.wasm$/i,
                    use: ["file-loader"],
                },
            ],
        },
        plugins: [
            new CopyWebpackPlugin({
                patterns: [{ from: "LICENSE*" }, { from: "README.md" }],
            }),
        ],
    };
};
