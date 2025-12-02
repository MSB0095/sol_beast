const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const webpack = require('webpack');
const fs = require('fs');
const CopyPlugin = require('copy-webpack-plugin');

const isProduction = process.env.NODE_ENV === 'production';
// Allow base path to be configured via environment variable for easy deployment to different repositories
const BASE_PATH = process.env.BASE_PATH || '/sol_beast/';

module.exports = {
  mode: isProduction ? 'production' : 'development',
  entry: './src/main.tsx',
  output: {
    path: path.resolve(__dirname, 'dist'),
    filename: 'assets/[name]-[contenthash].js',
    chunkFilename: 'assets/[name]-[contenthash].js',
    publicPath: isProduction ? BASE_PATH : '/',
    clean: true,
    module: true,
    environment: {
      module: true,
      dynamicImport: true,
    },
  },
  experiments: {
    outputModule: true,
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js', '.jsx'],
    extensionAlias: {
      '.js': ['.js', '.ts', '.tsx'],
    },
    fallback: {
      // Node.js polyfills for browser
      buffer: require.resolve('buffer/'),
      stream: require.resolve('stream-browserify'),
      util: require.resolve('util/'),
      process: require.resolve('process/browser.js'),
      crypto: false,
      path: false,
      fs: false,
    },
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: {
          loader: 'ts-loader',
          options: {
            configFile: 'tsconfig.webpack.json',
          },
        },
        exclude: /node_modules/,
      },
      {
        test: /\.css$/,
        use: ['style-loader', 'css-loader', 'postcss-loader'],
      },
      {
        test: /\.(png|svg|jpg|jpeg|gif)$/i,
        type: 'asset/resource',
      },
    ],
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: './index.html',
      inject: 'body',
      scriptLoading: 'module',
    }),
    new webpack.ProvidePlugin({
      Buffer: ['buffer', 'Buffer'],
      process: 'process/browser',
    }),
    new webpack.DefinePlugin({
      'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV || 'development'),
      'import.meta.env.VITE_USE_WASM': JSON.stringify(process.env.VITE_USE_WASM || 'false'),
      'import.meta.env.MODE': JSON.stringify(isProduction ? 'production' : 'development'),
      'import.meta.env.DEV': JSON.stringify(!isProduction),
      'import.meta.env.PROD': JSON.stringify(isProduction),
      'import.meta.env.BASE_URL': JSON.stringify(isProduction ? BASE_PATH : '/'),
      global: 'globalThis',
    }),
    new webpack.NormalModuleReplacementPlugin(/^process\/browser$/, 'process/browser.js'),
    new CopyPlugin({
      patterns: [
        {
          from: 'public',
          to: '.',
          globOptions: {
            ignore: ['**/index.html'], // index.html is handled by HtmlWebpackPlugin
          },
        },
      ],
    }),
    {
      // Plugin to create .nojekyll file for GitHub Pages
      apply: (compiler) => {
        compiler.hooks.afterEmit.tap('CreateNoJekyllPlugin', () => {
          const distPath = path.resolve(__dirname, 'dist');
          fs.writeFileSync(path.join(distPath, '.nojekyll'), '');
        });
      },
    },
  ],
  devServer: {
    static: './dist',
    port: 3000,
    hot: true,
    historyApiFallback: true,
  },
  optimization: {
    splitChunks: {
      chunks: 'all',
      cacheGroups: {
        vendor: {
          test: /[\\/]node_modules[\\/]/,
          name: 'vendors',
          priority: -10,
        },
        solana: {
          test: /[\\/]node_modules[\\/]@solana[\\/]/,
          name: 'solana-web3',
          priority: 10,
        },
        walletAdapter: {
          test: /[\\/]node_modules[\\/]@solana[\\/]wallet-adapter/,
          name: 'wallet-adapter',
          priority: 20,
        },
      },
    },
  },
};
