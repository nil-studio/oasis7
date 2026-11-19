ARG UBUNTU_BASE
ARG NODE_BASE=node:24-bookworm-slim
FROM ${NODE_BASE} AS node-tools

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

FROM runtime AS browser
COPY --from=node-tools /usr/local/bin/node /usr/local/bin/node
COPY --from=node-tools /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
COPY --from=browser-tools package.json package-lock.json /opt/oasis7-browser/
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/oasis7-playwright
ENV OASIS7_PLAYWRIGHT_MODULE=/opt/oasis7-browser/node_modules/playwright
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && npm ci --prefix /opt/oasis7-browser --ignore-scripts \
    && node "$OASIS7_PLAYWRIGHT_MODULE/cli.js" install chromium --with-deps --only-shell \
    && rm -rf /var/lib/apt/lists/* /root/.npm
