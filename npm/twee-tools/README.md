# @ifloom/twee-tools

A Twee compiler, story checker and media compressor for [Twine](https://twinery.org) stories, in one program: it
builds Twee as Tweego does, with SugarCube, Harlowe, Chapbook and Snowman built in, and checks the whole story every
time it builds.

This package installs twee-tools into a Node project, so the version is in your lock file and everyone on the
project gets the same one:

```sh
npm install --save-dev @ifloom/twee-tools
npx twee-tools build
```

npm installs the build for your system from its own package, such as `@ifloom/twee-tools-linux-x64`. For a global
install without Node, and everything twee-tools does, see [ifloom.dev/twee-tools](https://ifloom.dev/twee-tools) and
the [README on GitHub](https://github.com/IFLoom/twee-tools#readme).

Licensed under MIT or Apache-2.0, at your choice.
