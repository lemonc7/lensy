package main

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/lemonc7/lensy/service"
	"golang.org/x/term"
)

func handlePasswordCommand(args []string) (bool, error) {
	if len(args) == 0 {
		return false, nil
	}
	if args[0] == "password" {
		if len(args) != 1 {
			return true, errors.New("password 命令不接受其他参数")
		}
		password, err := readPassword()
		if err != nil {
			return true, err
		}
		return true, printPasswordHash(password)
	}

	var password string
	switch {
	case args[0] == "-password":
		if len(args) != 2 {
			return true, errors.New("-password 参数后必须提供一个密码")
		}
		password = args[1]
	case strings.HasPrefix(args[0], "-password="):
		if len(args) != 1 {
			return true, errors.New("-password 参数只能提供一个密码")
		}
		password = strings.TrimPrefix(args[0], "-password=")
	default:
		return true, errors.New("不支持的命令参数；生成密码哈希请使用 lensy password")
	}
	if password == "" {
		return true, errors.New("密码不能为空")
	}
	fmt.Fprintln(os.Stderr, "警告：命令行密码可能出现在 Shell 历史和进程列表中，建议使用 lensy password")
	return true, printPasswordHash(password)
}

func readPassword() (string, error) {
	stdinFD := int(os.Stdin.Fd())
	if !term.IsTerminal(stdinFD) {
		scanner := bufio.NewScanner(os.Stdin)
		if !scanner.Scan() {
			if err := scanner.Err(); err != nil {
				return "", fmt.Errorf("读取密码: %w", err)
			}
			return "", errors.New("未读取到密码")
		}
		return strings.TrimSuffix(scanner.Text(), "\r"), nil
	}

	fmt.Fprint(os.Stderr, "请输入密码: ")
	password, err := term.ReadPassword(stdinFD)
	fmt.Fprintln(os.Stderr)
	if err != nil {
		return "", fmt.Errorf("读取密码: %w", err)
	}
	fmt.Fprint(os.Stderr, "请再次输入密码: ")
	confirmation, err := term.ReadPassword(stdinFD)
	fmt.Fprintln(os.Stderr)
	if err != nil {
		return "", fmt.Errorf("读取确认密码: %w", err)
	}
	if string(password) != string(confirmation) {
		return "", errors.New("两次输入的密码不一致")
	}
	return string(password), nil
}

func printPasswordHash(password string) error {
	hash, err := service.HashPassword(password)
	if err != nil {
		return err
	}
	fmt.Println(hash)
	return nil
}
