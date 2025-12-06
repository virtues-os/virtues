# Custom PostgreSQL image with PostGIS and pgvector extensions
# Used by Virtues for geospatial data and vector embeddings

FROM postgis/postgis:16-3.4-alpine

# Install build dependencies for pgvector
RUN apk add --no-cache --virtual .build-deps \
    git \
    build-base \
    clang15 \
    llvm15-dev

# Build and install pgvector from source
RUN git clone --branch v0.8.0 https://github.com/pgvector/pgvector.git /tmp/pgvector \
    && cd /tmp/pgvector \
    && make \
    && make install \
    && rm -rf /tmp/pgvector

# Clean up build dependencies to reduce image size
RUN apk del .build-deps
