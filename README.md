# CS4555 Compiler Construction Project - Group 1

## Team Members
* Andreas Tsatsanis
* Konstantin Mirinski
* Panagiotis Panagi


**Responsible TA**: Charlie Ciaś

## How To Run The Tool

To run our tool with any example program from the `/examples` directory, simply run the following command with the name the example file of your choice:

```shell
cargo run --bin analyse -- examples/file_name.kap
```

To run the test suite of the project run:
```shell
cargo test
```

To specifically generate and visualize the Dot file of a program's CFG run:
```shell
cargo run --bin visualize -- examples/file_name.kap

dot -Tpng cfg.dot -o cfg.png
```

To evaluate an example program with our custom interpreter, run:
```shell
cargo run --bin run -- examples/file_name.kap
```

To set-up the LSP for visualizing the tool's results better, its binary can be found under `target/debug/lsp` or `target/release/lsp`.

## Input:
Example input programs utilizing our grammar can be found in `/examples`. Apart from that, here is a detailed description of the input language:

Our target language is inspired by Rust itself in both how it is formulated and in how it is expression-centric. Syntactic similarities of these two include that all expressions, declarations, and assignments end with a semicolon (;), that functions, if, and while statements need their bodies wrapped in curly braces ({}), that the later two's conditions do not need to be wrapped in parenthesis, that variable decelerations occur with the "let" keyword, that comments are wrapped between either "/*"-"*\" or "//"-"\n", that whitespace and indentations are ignored, and that the last expression of a block is assumed to be its return value and does not need to end in a semicolon. This allowed us to easily create mock programs based on this grammar to test our Rust project without having to think in a language that is vastly different from out implementation language.

Semantically, an inputted program is expected to abide by the following: A program contains at least one function. A function comprises a set of parameters (identifiers followed by a type and separated with commas) and an expression body. This body is usually a block but can be a simple arithmetic expression too, for example. That is because blocks contain zero or more statements, possibly followed by an expression that is not proceeded with a semicolon (the block's return value). On the contrary, statements include anything that has to be followed by a semicolon, whether that is a variable declaration, an assignment, or an expression. As discussed perviously, decelerations start with the "let" keyword and are followed by an identifier, its type, and an assignment to the identifiers initial expression value, while assignments are similar but do not require the "let" keyword or a type declaration.

Nonetheless, this all comes down to expressions: As expected, these are any combinations of and, or, and xor on equalities, which compare (using the standard comparison operators) arithmetic expressions of addition, subtraction, multiplication, and division on unary expressions. These refer to raising something to an exponent or negating an arithmetic expression. Still, these are applied to "primaries" - the lowest grammatical form in our language. For example, they refer to integers, booleans, identifiers, or function calls. Additionally, they can refer to if statements, while loops, blocks, or parenthesized expressions, thus forming the recursive definition of the grammar. Importantly, separating expressions and statements, allows users to add, for example, integers with if statements, but not integers with other declarations or assignments, as those are segregated in a higher level.

## Output:
The output of the **analyse** command is the source code of the inputted programs with all available expressions being explicitly highlighted. Thus, if you see a highlighted expression that means it was needlessly recalculated there.