resource "aws_instance" "app1" {
  instance_type = "t3.medium"
  ami = "ami-0c55b159cbfafe1f0"
  subnet_id = aws_subnet.private_a.id
}

resource "aws_instance" "web1" {
  instance_type = "t3.small"
  ami = "ami-0c55b159cbfafe1f0"
  subnet_id = aws_subnet.public_a.id
}

resource "aws_instance" "web2" {
  instance_type = "t3.small"
  ami = "ami-0c55b159cbfafe1f0"
  subnet_id = aws_subnet.public_b.id
}
