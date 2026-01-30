# Python Sandbox Container
# Used for secure code execution in both dev (OrbStack) and prod (gVisor)

FROM python:3.12-slim-bookworm

# Install bubblewrap for process-level sandboxing
RUN apt-get update && apt-get install -y --no-install-recommends \
    bubblewrap \
    && rm -rf /var/lib/apt/lists/*

# Install Python packages for personal AI use cases:
# - Data analysis: numpy, pandas, scipy
# - Visualization: matplotlib
# - Finance: numpy-financial (PMT, FV, PV, IRR, NPV)
# - Utilities: requests, python-dateutil, pytz
RUN pip install --no-cache-dir \
    numpy \
    numpy-financial \
    pandas \
    matplotlib \
    scipy \
    requests \
    python-dateutil \
    pytz

# Create sandbox user (non-root)
RUN useradd -m -s /bin/bash sandbox

# Create workspace directory
RUN mkdir -p /workspace && chown sandbox:sandbox /workspace

# Copy the entrypoint script
COPY sandbox-entrypoint.sh /usr/local/bin/sandbox-entrypoint.sh
RUN chmod +x /usr/local/bin/sandbox-entrypoint.sh

USER sandbox
WORKDIR /workspace

ENTRYPOINT ["/usr/local/bin/sandbox-entrypoint.sh"]
