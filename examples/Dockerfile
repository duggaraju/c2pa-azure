# Use a Rust base image with Cargo installed
FROM rust AS builder

# Set the working directory inside the container
WORKDIR /usr/src/app

# Copy the current directory contents into the container at /usr/src/myapp
COPY . .

# Build the Rust workspace
RUN cargo build --release

# Start a new stage to create a smaller image without unnecessary build dependencies
FROM ubuntu AS runtime

RUN apt-get update && apt-get install -y libssl3 ca-certificates

# Set the working directory
WORKDIR /usr/local/bin

FROM runtime AS function
# Copy the built binary from the previous stage
COPY --from=builder /usr/src/app/target/release/function ./function

# Command to run the application
ENTRYPOINT ["function"]
LABEL name=function

FROM runtime AS keda-blob-storage
# Copy the built binary from the previous stage
COPY --from=builder /usr/src/app/target/release/keda-blob-storage ./

# Command to run the application
ENTRYPOINT ["keda-blob-storage"]
LABEL name=keba-blob-storage
