# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

Only the latest release is actively supported with security updates.

## Reporting a Vulnerability

If you discover a security vulnerability in Quay, please report it through GitHub's **Private vulnerability reporting** feature:

1. Go to the [Security tab](https://github.com/shonenm/quay/security) of this repository
2. Click **"Report a vulnerability"**
3. Fill in the details of the vulnerability

**Please do NOT open a public issue for security vulnerabilities.**

## Response Process

1. **Acknowledgment**: We will acknowledge receipt of your report within 48 hours
2. **Assessment**: We will assess the severity and impact of the vulnerability
3. **Fix**: A fix will be developed and tested
4. **Release**: A patched version will be released with a security advisory
5. **Disclosure**: The vulnerability will be publicly disclosed after the fix is available

## Scope

This policy covers the Quay CLI tool and its dependencies. Security issues in the following areas are in scope:

- Command injection via port/process data
- Unsafe handling of SSH credentials or connection parameters
- Dependency vulnerabilities (monitored via `cargo-audit` in CI)
