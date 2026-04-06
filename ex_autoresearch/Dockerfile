# ── Build Stage ──────────────────────────────────────────────────────
FROM hexpm/elixir:1.18-otp-27 AS builder

ENV MIX_ENV=prod \
    LANG=C.UTF-8 \
    LANGUAGE=en_US

RUN apt-get update -y && \
    apt-get install -y build-essential git inotify-tools npm && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Hex + Rebar3
RUN mix local.hex --force && mix local.rebar --force

# Install and compile deps
COPY mix.exs mix.lock ./
RUN mix deps.get --only ${MIX_ENV} && \
    mix deps.compile

# Copy source
COPY config config/
COPY lib lib/
COPY priv priv/
COPY assets assets/

# Build assets
RUN cd assets && npm install
RUN mix assets.deploy

# Compile the app
RUN mix compile

# Generate release
COPY rel rel/
RUN mix release

# ── Runtime Stage ─────────────────────────────────────────────────────
FROM debian:bookworm-slim

ENV LANG=C.UTF-8 \
    LANGUAGE=en_US \
    MIX_ENV=prod \
    HOME=/app

RUN apt-get update -y && \
    apt-get install -y libstdc++6 openssl ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy release from builder
COPY --from=builder /app/_build/${MIX_ENV}/rel/ex_autoresearch ./

RUN chown -R nobody: /app
USER nobody

ENTRYPOINT ["bin/ex_autoresearch"]
CMD ["start"]
