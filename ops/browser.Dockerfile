FROM mcr.microsoft.com/playwright:v1.64.1-noble@sha256:7852dc5a7d72f5fe458a597d690f6c42fbc8e9e26ddfa95f8e6ef552558d9bbf
RUN apt-get update && apt-get install -y --no-install-recommends libnss3-tools socat && rm -rf /var/lib/apt/lists/*
COPY ops/browser.sh /usr/local/bin/miy-browser
ENTRYPOINT ["bash", "/usr/local/bin/miy-browser"]
