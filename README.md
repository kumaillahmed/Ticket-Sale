# Selling Tickets Online (Swiftly)


## Structure

The template is used for both Java and Rust. It is structured as follows:

- `crates/`: Rust code
  - `ticket-sale-core`: Infrastructure for working with requests
  - `ticket-sale-rocket`: The implementation 
  - `ticket-sale-server`: HTTP server binary using the implementation
  - `ticket-sale-tests`: Automated integration tests (can be used for Java as well)
- `src/`: Java code
  - `main/java/com/pseuco/cp24/`: Java source code of the project
      - `request`: Infrastructure for working with requests
      - `rocket`: The implementation
      - `slug`: A sequential reference implementation
  - `test`: Unit tests


## Running the Project

### Java ☕

We use [Gradle](https://gradle.org/) to build the project. You can interact with Gradle from the terminal. The most important subcommands are:

- `./gradlew jar` builds the project as `out/cli.jar`.
- `./gradlew run` compiles and starts the ticket sales system.
- `./gradlew javaDoc` builds the Javadoc in `build/docs/`.
- `./gradlew test` runs the unit tests

Once you have built the JAR file, you can also run it without Gradle and supply the command line flags listed in the project description. For instance, the following command will start the sequential slug implementation: ```java -ea -jar out/cli.jar -slug```

Running (and debugging) the project directly from VS Code without the commandline is also possible.


### Rust 🦀

If you use Rust, Cargo is your friend:

- `cargo run -p ticket-sale-server` compiles and starts the ticket sales system.
- `cargo doc` generates API documentation. `cargo doc --open` additionally opens the generated documentation in a web browser.
- `cargo test -p ticket-sale-rocket` runs the unit tests you write inside the implementation.

To pass flags to the implementation, e.g., to execute the slug implementation, append them after a `--`: ```sh cargo run -p ticket-sale-server -- -slug```


### Test Infrastructure

Our automated test infrastructure is written in Rust. To add tests, place them in `crates/ticket-sale-tests/tests/`. We provide an example test in `example.rs`. You may create other files in this directory. Every test file may contain multiple test cases, which are Rust functions decorated with `#[tokio::test]`. For a general description of how to write tests in Rust, you may want to have a look at [Chapter 11 of the Rust Book](https://doc.rust-lang.org/stable/book/ch11-00-testing.html).

Running the test suite is possible via ```sh cargo test -p ticket-sale-tests --tests -- --show-output```

If you use Java, please note that this will not build your Java project. You need to run `./gradlew jar` beforehand. If you are using Rust, Cargo takes care of recompiling the project.

If you want to test the implementation’s performance, build the tests (as well as the project) in release mode. To this end, just add the `--release` flag to the command above: ```sh cargo test -p ticket-sale-tests --release --tests -- --show-output```


### For Convenience: A `justfile` 🎉

Some of the commands above are a bit lengthy and may be hard to remember, some even depend on other commands. To make your life a bit easier, we provide a [`justfile`](https://just.systems/man/en/) with a simpler interface. To install `just` via Cargo, run `cargo install just`. A complete list of commands provided by our `justfile` can be obtained via `just help`:

    Available recipes:
    build      # Build the project
    doc        # Generate API documentation
    doc-open   # Generate API documentation and open it in the web browser
    help       # Print available recipes
    lang       # Get the detected programming language
    lint       # Run clippy on Rust code
    spellcheck # Run CSpell
    test       # Run the tests in crates/ticket-sale-tests/tests (use `just release=1 test` to benchmark the Rust implementation)

For example, you can pass the command line options specified in the project description directly to `just run`, e.g., `just run -tickets 1337` to start the server with 1337 initially available tickets.

`just` can automatically detect the programming language. However, this requires Python ≥ 3.11 to be installed and available via `python3`. If this is not the case on your system, you can select the language manually, for example: `just lang=rust test` or `just lang=java test`.


## Rust-Specific Remarks

The Rust template roughly follows the structure of the Java template.

We use the following Rust libraries:

- [`parking_lot`](https://docs.rs/parking_lot/latest/parking_lot/index.html) for mutexes without lock poisoning
- [`rand`](https://docs.rs/rand/latest/rand/index.html) to get random values
- [`uuid`](https://docs.rs/uuid/latest/uuid/index.html) for server and customer ids

You may assume that the safe interfaces provided by these crates are sound (i.e., no matter how you use them within Safe Rust, you will not cause data races or other kinds of undefined behavior). You are allowed to add other crates as dependencies, but you take full responsibility for soundness issues within them.
