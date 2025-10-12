FROM python:latest

WORKDIR /app

ADD server.py .
ADD requirements.txt .

RUN ["pip3", "install", "-r", "./requirements.txt"]

# expose metrics
EXPOSE 7004/tcp
# receive data
EXPOSE 7004/udp

#ENTRYPOINT ["/usr/bin/python3", "server.py"]
ENTRYPOINT ["/usr/local/bin/python", "server.py"]
