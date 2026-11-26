#!/usr/bin/env node
// The `twee-tools` command npm installs: runs the twee-tools binary for this system, which npm installed beside it from
// its own package, @ifloom/twee-tools-<system>, chosen by that package's os and cpu.
'use strict'

const { spawnSync } = require('node:child_process')

/** Each system, `platform arch` as Node names it, and the package with its build. */
const PACKAGES = {
  'linux x64': '@ifloom/twee-tools-linux-x64',
  'linux arm64': '@ifloom/twee-tools-linux-arm64',
  'darwin x64': '@ifloom/twee-tools-darwin-x64',
  'darwin arm64': '@ifloom/twee-tools-darwin-arm64',
  'win32 x64': '@ifloom/twee-tools-win32-x64',
  // Windows on Arm runs the x64 build.
  'win32 arm64': '@ifloom/twee-tools-win32-x64',
}

function fail(message) {
  console.error(`twee-tools: ${message}`)
  process.exit(1)
}

const name = PACKAGES[`${process.platform} ${process.arch}`]
if (!name) fail(`there's no build for ${process.platform} on ${process.arch}. See https://ifloom.dev/twee-tools for other ways to install it.`)

let binary
try {
  binary = require.resolve(`${name}/bin/twee-tools${process.platform === 'win32' ? '.exe' : ''}`)
} catch {
  fail(
    `${name} isn't installed. npm installs it with @ifloom/twee-tools, unless optional dependencies are turned off ` +
      '(--omit=optional, --no-optional): install again without that.',
  )
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' })
if (result.error) fail(`couldn't run ${binary}: ${result.error.message}`)
// Ended by a signal (Ctrl+C in `twee-tools dev`): end the same way.
if (result.signal) process.kill(process.pid, result.signal)
process.exit(result.status ?? 1)
