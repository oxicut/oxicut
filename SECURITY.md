# Security

## Reporting a problem

If you find a security problem in OxiCut, please report it privately, so it can be fixed before it is public:

1. Open the [Security tab](https://github.com/oxicut/oxicut/security) of this repository.
2. Choose **Report a vulnerability**, and describe what you found and how to reproduce it.

Please do not open a public issue for it. Ordinary bugs (something does not work) belong in the [issues](https://github.com/oxicut/oxicut/issues).

## What counts

The program and its installer, the downloads it fetches (the models), the project files it reads (`.oxicut`), and the quiet check for a newer version. Examples: a crafted scan, project file or download that makes the program run something it should not, or write outside the folders it was told to use.

OxiCut never changes your original scans, and apart from the downloads you start and the check for a newer version (which can be switched off in Settings) it does not use the network.

## What to expect

You will hear back within about a week. OxiCut is a small project run by volunteers, so fixes come as soon as they can, and a fix is released as a new version that is listed in the [changelog](CHANGELOG.md). Only the newest release is supported.
