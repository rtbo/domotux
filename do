#!/usr/bin/env bash

set -e

# Utility to work remotely on Remote Host
# This script is meant to be uploaded to the remote host and used there or through SSH.
CD=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

stop_service() {
        if [[ -z $1 ]];
        then
                echo -e "\e[1;31mPackage name must be provided\e[0m" >&2
                exit 1
        fi
        echo -e "\e[1;32mStopping $1 service\e[0m"

        sudo systemctl stop $1.service
}

start_service() {
        if [[ -z $1 ]];
        then
                echo -e "\e[1;31mPackage name must be provided\e[0m" >&2
                exit 1
        fi
        echo -e "\e[1;32mStarting $1 service\e[0m"

        sudo systemctl start $1.service
}

install_service() {
        if [[ -z $1 ]];
        then
                echo -e "\e[1;31mPackage name must be provided\e[0m" >&2
                exit 1
        fi

        echo -e "\e[1;32mInstalling $1 service\e[0m"

        sudo install ${CD}/$1 /usr/local/bin/$1
        sudo install ${CD}/$1.yml /etc/$1.yml
        sudo install ${CD}/$1.service /etc/systemd/system/$1.service
        sudo systemctl stop $1.service || true
        sudo systemctl daemon-reload
        sudo systemctl enable $1.service
}

restart_service() {
        if [[ -z $1 ]];
        then
                echo -e "\e[1;31mPackage name must be provided\e[0m" >&2
                exit 1
        fi

        echo -e "\e[1;32mRestarting $1 service\e[0m"

        sudo systemctl restart $1.service
}

while [ $# -gt 0 ]
do
        case "$1" in
                stop)
                        stop_service $2
                        shift
                        ;;
                start)
                        start_service $2
                        shift
                        ;;
                install)
                        install_service $2
                        shift
                        ;;
                restart)
                        restart_service $2
                        shift
                        ;;
                *)
                        echo -e "\e[1;31mUnknown command: $1\e[0m" >&2
                        echo "Usage: do [command]" >&2
                        echo "Available commands: stop, start, install" >&2
                        exit 1
                        ;;
        esac
        shift
done
