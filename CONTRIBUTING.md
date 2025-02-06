# Contributing to dukascopy-fx

Thank you for your interest in contributing to `dukascopy-fx`! We welcome contributions from the community to help improve this library. Whether you're fixing a bug, adding a feature, or improving documentation, your contributions are highly appreciated.

Please take a moment to review this document to ensure a smooth and efficient contribution process.

---

## Table of Contents
- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
  - [Setting Up the Project](#setting-up-the-project)
  - [Running Tests](#running-tests)
- [Reporting Issues](#reporting-issues)
- [Submitting Pull Requests](#submitting-pull-requests)
- [Code Style and Guidelines](#code-style-and-guidelines)
- [License](#license)

---

## Code of Conduct

By participating in this project, you agree to abide by our Code of Conduct. Please read it before contributing.

---

## Getting Started

### Setting Up the Project

#### 1. Fork the Repository

Click the "Fork" button on the top right of the repository page to create your own copy.

#### 2. Clone the Repository

```bash
git clone https://github.com/Kluzko/dukascopy-fx.git
cd dukascopy-fx
```

#### 3. Install Dependencies

Ensure you have Rust installed. If not, follow the instructions at [rustup.rs](https://rustup.rs/).

```bash
cargo build
```

#### 4. Set Up Your Development Environment

- Install `rust-analyzer` for your editor (e.g., VSCode) for better IDE support.
- Install `cargo-watch` for automatic recompilation during development:

```bash
cargo install cargo-watch
```

---

### Running Tests

To ensure your changes don't break existing functionality, run the tests before submitting a pull request.

#### Run Unit Tests:

```bash
cargo test
```

#### Run Integration Tests:

```bash
cargo test --test integration_test
```

#### Run Examples:

```bash
cargo run --example basic
```

---

## Reporting Issues

If you encounter a bug or have a feature request, please open an issue on GitHub. Follow these guidelines to ensure your issue is addressed quickly:

1. **Check Existing Issues**
   Before creating a new issue, search the issue tracker to see if it has already been reported.

2. **Provide Detailed Information**
   - Include a clear and descriptive title.
   - Describe the problem or feature request in detail.
   - Provide steps to reproduce the issue (if applicable).
   - Include error messages, logs, or screenshots (if applicable).

3. **Use the Issue Template**
   Fill out the issue template provided when creating a new issue.

---

## Submitting Pull Requests

We welcome pull requests! Follow these steps to submit your changes:

### 1. Create a Branch

```bash
git checkout -b my-feature-branch
```

### 2. Make Your Changes

- Write your code and ensure it follows the **Code Style and Guidelines**.
- Add tests for new functionality or bug fixes.
- Update the documentation if necessary.

### 3. Run Tests

Ensure all tests pass before submitting your pull request.

### 4. Commit Your Changes

- Write clear and concise commit messages.
- Reference any related issues in your commit messages (e.g., `Fixes #123`).

### 5. Push Your Changes

```bash
git push origin my-feature-branch
```

### 6. Open a Pull Request

- Go to the pull requests page and click "New Pull Request."
- Fill out the pull request template and provide a detailed description of your changes.

---

## Code Style and Guidelines

To maintain consistency across the codebase, please follow these guidelines:

### 1. Format Your Code

Use `cargo fmt` to format your code according to Rust's style guidelines.

```bash
cargo fmt
```

### 2. Lint Your Code

Use `cargo clippy` to catch common mistakes and improve code quality.

```bash
cargo clippy
```

### 3. Write Tests

- Add **unit tests** for new functionality.
- Add **integration tests** for end-to-end functionality.

### 4. Document Your Code

- Add documentation comments (`///`) for public functions, structs, and modules.
- Update the `README.md` if your changes affect the library's usage.

### 5. Keep Commits Small and Focused

- Each commit should address a single issue or feature.
- Avoid large, monolithic commits.

---

## License

By contributing to this project, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing to `dukascopy-fx`! Your efforts help make this library better for everyone.

---

### Additional Notes

- **License**: Ensure your repository has a LICENSE file (e.g., MIT License) before submitting it.
- **Issues**: Use GitHub's issue templates to standardize issue reporting. You can create templates for bugs, feature requests, and questions.
