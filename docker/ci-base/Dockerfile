ARG UBUNTU_BASE
FROM ${UBUNTU_BASE} AS runtime
COPY runtime-packages.txt /tmp/ci-packages.txt
RUN apt-get update \
    && xargs apt-get install -y --no-install-recommends </tmp/ci-packages.txt \
    && rm -rf /var/lib/apt/lists/* /tmp/ci-packages.txt

FROM runtime AS build
COPY build-packages.txt /tmp/ci-packages.txt
RUN apt-get update \
    && xargs apt-get install -y --no-install-recommends </tmp/ci-packages.txt \
    && rm -rf /var/lib/apt/lists/* /tmp/ci-packages.txt
