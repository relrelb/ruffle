/* eslint-env node */

const path = require("path");

module.exports = (env, argv) => {
    let mode = "production";
    if (argv && argv.mode) {
        mode = argv.mode;
    }

    console.log(`Building ${mode}...`);

    return {
        mode,
        entry: "./src/index.ts",
        output: {
            path: path.resolve(__dirname, "dist"),
            filename: "index.js",
            publicPath: "",
            clean: true,
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
                    use: ["file-loader"],
                },
            ],
        },
        resolve: {
            extensions: [".ts", ".js"],
        },
        devtool: "source-map",
    };
};
