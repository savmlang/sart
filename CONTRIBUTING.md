# Contributing to Sa VM

Thank you for your interest in the Sa VM Runtime organization. By contributing, you are making the "Sa Ecosystem" better as a whole - increased stability and victory for all!

## ⚖️ Licensing Agreement

By contributing to this repository, you agree to the following:

- **Primary License**: Your **contributions** are licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

- **Ecosystem Exception:** You grant **[other entities](#other-entities)** an exclusive or non-exclusive, perpetual, irrevocable, worldwide license to use, modify, and distribute your contributions under the **MIT License or Apache License 2.0** specifically for use with due permission from **AHQ Softwares**.

[!IMPORTANT] **Policy Notice:** This contribution policy and the associated licensing exceptions are subject to change. We reserve the right to update these terms to better protect the project or comply with future research goals. Please check this file periodically for updates.

### Other Entities

The entities involve the following:

- AHQ Softwares (https://github.com/ahq-softwares)
- AHQ Store (https://github.com/ahqstore)

**3rd Party Exclusions:** Other 3rd party entities may be granted access to the code under alternative terms, or explicitly restricted, as detailed in our exclusion registry: https://github.com/savmlang/exclusions.

## 🛠️ Contribution Guidelines

- **GPG Signing Required:** All commits must be signed. This ensures the integrity of the Sa ecosystem and prevents unauthorized modifications to the core logic.

- **Performance First:** Sa VM is designed to be efficient. Please include benchmarks or performance considerations for changes to the JIT, GC, or Assembler—especially for lower-spec hardware targets.

- **Atomic Commits:** Keep your Pull Requests focused. If you are fixing "broken assembly" and also adding a feature, please separate them into distinct PRs.

- **Unsafe code:** `unsafe` Rust is encouraged whenever the "Safe" alternative introduces **runtime overhead (extra instructions/checks)** or **architectural overhead (unnecessary complexity to satisfy the borrow checker)**. We prioritize a lean, readable, lab-tested safe and fast VM. However, every `unsafe` block must include a `// SAFETY: ` comment explaining why the operation is sound along with tests for potential edge-cases.
