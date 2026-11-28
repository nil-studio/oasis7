FROM ghcr.io/nil-studio/oasis7/ci-api-runtime@sha256:ba7c763f7b634b672c6dccef1384bce4daf7abd44a65d613696a748891e3c0ed AS api-base

FROM api-base AS sdk-builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential python3-dev \
    && rm -rf /var/lib/apt/lists/*
COPY artifacts.json prepare.py /opt/oss-build/
RUN python3 /opt/oss-build/prepare.py fetch /tmp/oss-distributions
RUN --network=none python3 /opt/oss-build/prepare.py install /tmp/oss-distributions

FROM api-base AS ecs-oss-publisher
COPY --from=sdk-builder /opt/oasis7-oss /opt/oasis7-oss
COPY artifacts.json smoke.py /opt/oasis7-oss-proof/
ENV PATH=/opt/oasis7-oss/bin:$PATH
ENV PYTHONNOUSERSITE=1
