### clone
```angular2html
git clone https://github.com/samiyonas/tcpchat
```

### create self signed certificate and secret key for secure communication using tls protocol
```angular2html
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes 
```

### compile
```angular2html
rustc src/main.rs
```

### run
#### terminal 1
```angular2html
./main
```
#### terminal 2
```angular2html
telnet 127.0.0.1 6969
```
#### terminal 3
```angular2html
telnet 127.0.0.1 6969
```

you can now chat between terminal 2 and terminal 3