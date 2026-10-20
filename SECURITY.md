# Security Policy

## Supported versions

AetherCast is in the research phase. There are no supported releases yet. This policy will be updated before the first public release.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Use GitHub's private vulnerability reporting for this repository:

1. Open the **Security** tab.
2. Select **Report a vulnerability**.
3. Include the affected version or commit, reproduction steps, impact, and any suggested fix.

If private reporting is unavailable, contact the maintainer, [@birukyitayew](https://github.com/birukyitayew), through GitHub to request a private channel.

## In scope

- Memory-safety bugs in protocol parsers or media pipelines
- Authentication or pairing bypasses
- Credential, token, or private-screen-data leakage
- Malicious sink or discovery responses that crash or compromise the client
- Packaging or update-chain issues
- Sandbox or permission escapes

## Out of scope

- DRM/HDCP circumvention requests
- Vulnerabilities in third-party TVs, dongles, routers, or drivers that are not caused by AetherCast
- Denial of service that requires physical access to the machine

## Disclosure

We will acknowledge a report as soon as possible, investigate, and coordinate a disclosure timeline with the reporter. Please give us a reasonable opportunity to fix the issue before publishing details.
