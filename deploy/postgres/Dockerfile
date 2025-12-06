# Custom PostgreSQL image with PostGIS and pgvector extensions
# Used by Virtues for geospatial data and vector embeddings

FROM postgis/postgis:16-3.4-alpine

# Install pgvector extension from Alpine edge repository
RUN apk add --no-cache --repository=https://dl-cdn.alpinelinux.org/alpine/edge/community pgvector
