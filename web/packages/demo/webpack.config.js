/* eslint-env node */

const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");

module.exports = (env, argv) => {
    let mode = "production";
    if (argv && argv.mode) {
        mode = argv.mode;
    }

    console.log(`Building ${mode}...`);

    return {
        mode,
        entry: path.resolve(__dirname, "www/index.js"),
        output: {
            path: path.resolve(__dirname, "dist"),
            filename: "index.js",
            publicPath: "",
            clean: true,
        },
        module: {
            rules: [
                {
                    test: /\.css$/i,
                    use: ["style-loader", "css-loader"],
                },
            ],
        },
        devtool: "source-map",
        plugins: [
            new CopyPlugin({
                patterns: [
                    {
                        from: "*.wasm",
                        context: path.resolve(
                            __dirname,
                            "node_modules/ruffle-core/dist/"
                        ),
                    },
                    { from: path.resolve(__dirname, "www/index.html") },
                    { from: "LICENSE*" },
                    { from: "README.md" },
                ],
            }),
        ],
    };
};
