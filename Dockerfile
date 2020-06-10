# our final base
FROM rust:slim-buster

# copy the build artifact from the build stage
COPY ./target/release/tinysearch .

# set the startup command to run your binary
CMD ["./tinysearch"]
