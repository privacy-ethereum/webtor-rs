# Consensus Diffs

This document outlines the mechanism of consensus diffs in the Tor network and the feasibility of implementing this feature in `webtor-rs`.

## How Consensus Diffs Work

The Tor directory protocol provides a mechanism to download consensus documents as diffs, which is a bandwidth-saving feature. Instead of downloading the full consensus document on every update, clients can request only the changes since their last known version.

The process is as follows:
1. A client fetches the full consensus for the first time.
2. From the response headers, the client stores the `Last-Modified` and `ETag` values.
3. On subsequent requests to the same endpoint, the client sends an `If-Modified-Since` header with the stored `Last-Modified` value.
4. The server then responds in one of three ways:
    - `304 Not Modified`: The consensus has not changed.
    - `200 OK` with a full consensus: The server decides to send the full document.
    - `200 OK` with a consensus diff: The server sends a diff that can be applied to the client's stored consensus.

## Detecting a Diff

A client can differentiate between a full consensus and a diff by inspecting the first line of the response body:
- A **full microdescriptor consensus** starts with `network-status-version 3 microdesc`.
- A **consensus diff** starts with `network-status-diff-from <base64-digest> <base64-digest>`.

Both full consensus documents and diffs are served from the same endpoint, for example `/tor/status-vote/current/consensus-microdesc`.

## Implementation in `webtor-rs`

The version of the `tor-netdoc` crate (`0.36.0`) used in this project does not appear to expose the necessary APIs to easily parse and apply consensus diffs (specifically, `ConsensusDiff` and `apply_diff`).

### Forking `tor-netdoc`

One possibility would be to fork the `tor-netdoc` crate to implement this functionality. However, `tor-netdoc` is part of the larger Arti project and has many dependencies on other crates within the Arti workspace.

Forking only `tor-netdoc` would require either copying all its dependencies or rewriting the `path` dependencies in its `Cargo.toml` to `git` dependencies, which would create a complex and fragile build process.

### Conclusion

Given the complexity of forking `tor-netdoc`, it is recommended to either contribute the required features to the upstream Arti project or wait for a newer version of `tor-netdoc` that includes this functionality. For now, `webtor-rs` will continue to fetch the full consensus document on every update.
