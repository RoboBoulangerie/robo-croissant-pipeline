FROM rust:trixie

ARG RC_MODEL=""
ENV RC_MODEL=$RC_MODEL

ARG RC_CLIENT_TYPE=""
ENV RC_CLIENT_TYPE=$RC_CLIENT_TYPE

ARG RC_CLIENT_API_KEY=""
ENV RC_CLIENT_API_KEY=$RC_CLIENT_API_KEY

ARG RC_TARGETED_KNOWLEDGE_SOURCES=""
ENV RC_TARGETED_KNOWLEDGE_SOURCES=$RC_TARGETED_KNOWLEDGE_SOURCES

ARG USERNAME=rc
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && apt-get update \
    && apt-get install -y sudo \
    && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
    && chmod 0440 /etc/sudoers.d/$USERNAME

RUN apt -y install pkg-config libssl-dev build-essential

RUN chown -R rc:rc /home/rc

USER $USERNAME

RUN cargo install aichat spider_cli nu

COPY . /home/rc/

WORKDIR /home/rc/

CMD nu /home/rc/main.nu



